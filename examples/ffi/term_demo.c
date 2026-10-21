/* Copyright © Fangdun Tsai <fundon@pindash.io>
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 *
 * term_demo.c — use vtty_ffi + a PTY as a minimal interactive terminal.
 *
 * Keystrokes from stdin are forwarded to a shell running on the PTY; the
 * shell's output is fed through the vtty parser and printed. Ctrl-D (or EOF
 * on stdin) ends the session.
 *
 * Build (macOS / BSD, from `examples/ffi/`):
 *   cargo build --release -p vtty_ffi
 *   cc -O2 term_demo.c -I ../../crates/vtty_ffi -L ../../target/release -lvtty_ffi -o term_demo
 *
 * Build (Linux, needs libutil for forkpty):
 *   cc -O2 term_demo.c -I ../../crates/vtty_ffi -L ../../target/release -lvtty_ffi -lutil -o term_demo
 *
 * Run: ./term_demo     (or pipe commands: printf 'ls\nexit\n' | ./term_demo)
 */

#include "vtty_ffi.h"

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) || \
    defined(__NetBSD__)
#include <util.h>
#else
#include <pty.h>
#endif

static struct termios saved_termios;
static int termios_saved;

/* Restore the parent terminal before exiting. */
static void restore_tty(void) {
    if (termios_saved) {
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &saved_termios);
    }
}

/* Put stdin into raw mode so keystrokes pass through immediately (no line
 * buffering, no local echo). No-op when stdin is not a terminal (e.g. a pipe). */
static void raw_mode(void) {
    if (tcgetattr(STDIN_FILENO, &saved_termios) == 0) {
        termios_saved = 1;
        struct termios t = saved_termios;
        t.c_lflag &= ~(tcflag_t)(ICANON | ECHO | ISIG);
        t.c_iflag &= ~(tcflag_t)(IXON | ICRNL);
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &t);
        atexit(restore_tty);
    }
}

int main(void) {
    /* Slave terminal: canonical line mode with echo and standard control
     * characters, so the shell echoes keystrokes and the parser sees the full
     * stream. Base the termios on the parent's (for the user's c_cc defaults)
     * and fall back to a minimal sane setup when stdin is not a terminal. */
    struct termios t;
    if (tcgetattr(STDIN_FILENO, &t) != 0) {
        memset(&t, 0, sizeof(t));
        t.c_cflag = B38400 | CS8 | CREAD;
        t.c_iflag = ICRNL;
        t.c_oflag = OPOST | ONLCR;
        t.c_cc[VEOF] = 0x04;   /* Ctrl-D */
        t.c_cc[VINTR] = 0x03;  /* Ctrl-C */
        t.c_cc[VERASE] = 0x7f; /* Backspace */
    }
    t.c_lflag = ICANON | ECHO | ISIG | IEXTEN;

    int master = -1;
    pid_t pid = forkpty(&master, NULL, &t, NULL);
    if (pid < 0) {
        perror("forkpty");
        return 1;
    }
    if (pid == 0) {
        execl("/bin/sh", "sh", (char *)NULL);
        _exit(127);
    }

    vt_state *vt = vt_new();
    if (vt == NULL) {
        fprintf(stderr, "vt_new failed\n");
        return 1;
    }
    raw_mode();

    /* Banner: show the ABI version before the shell takes over. */
    uint32_t ver = vt_version();
    printf("vtty_ffi %u.%u.%u (Ctrl-D to exit)\n", ver >> 16,
           (ver >> 8) & 0xffu, ver & 0xffu);

    char out[4096];
    char in[256];
    size_t printed = 0; /* bytes of vt_text already written to stdout */
    int stdin_open = 1;
    int stdin_is_tty = isatty(STDIN_FILENO);

    for (;;) {
        fd_set rfds;
        FD_ZERO(&rfds);
        if (stdin_open) {
            FD_SET(STDIN_FILENO, &rfds);
        }
        FD_SET(master, &rfds);
        int maxfd = (STDIN_FILENO > master ? STDIN_FILENO : master) + 1;

        /* Interactive sessions block indefinitely (no auto-exit). Only the
         * drain phase of a piped session uses a timeout, so it exits once the
         * shell is idle. */
        struct timeval tv = { .tv_sec = 1, .tv_usec = 0 };
        struct timeval *tvp = NULL;
        if (!stdin_open && !stdin_is_tty) {
            tvp = &tv;
        }

        int ready = select(maxfd, &rfds, NULL, NULL, tvp);
        if (ready < 0) {
            if (errno == EINTR) {
                continue;
            }
            perror("select");
            break;
        }
        if (ready == 0) {
            break; /* piped drain phase timed out */
        }

        /* Keystroke -> shell. */
        if (stdin_open && FD_ISSET(STDIN_FILENO, &rfds)) {
            ssize_t n = read(STDIN_FILENO, in, sizeof(in));
            if (n <= 0) {
                /* Ctrl-D on a real terminal ends the session. On a pipe, just
                 * stop forwarding and keep draining the shell's output. */
                if (stdin_is_tty) {
                    close(master);
                    break;
                }
                stdin_open = 0;
            } else if (memchr(in, 0x04, (size_t)n) != NULL) {
                /* Raw mode delivers Ctrl-D as the 0x04 byte: end the session
                 * without forwarding it to the shell. */
                close(master);
                break;
            } else {
                (void)write(master, in, (size_t)n);
            }
        }

        /* PTY output -> parser -> print only the newly parsed bytes. */
        if (FD_ISSET(master, &rfds)) {
            ssize_t n = read(master, out, sizeof(out));
            if (n <= 0) {
                break; /* shell exited */
            }
            vt_advance(vt, (const uint8_t *)out, (size_t)n);

            size_t len = vt_text_len(vt);
            const char *text = vt_text(vt);
            if (len > printed) {
                fwrite(text + printed, 1, len - printed, stdout);
                fflush(stdout);
                printed = len;
            }
        }
    }

    kill(pid, SIGKILL); /* reap the child if it is still running */
    waitpid(pid, NULL, 0);
    vt_free(vt);
    restore_tty();
    putchar('\n');
    return 0;
}
