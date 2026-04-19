#include <stdint.h>
#include <termios.h>
#include <unistd.h>
#include <stdio.h>
#include <sys/ioctl.h>

// Neovimの xpopcount と同等のロジック
unsigned xpopcount_test(uint64_t x) {
    unsigned count = 0;
    for (; x != 0; x >>= 1) {
        if (x & 1) {
            count++;
        }
    }
    return count;
}

static struct termios orig_termios;

void os_setup_terminal() {
    tcgetattr(STDIN_FILENO, &orig_termios);
    struct termios raw = orig_termios;
    // Input flags: disable break, CR to NL, parity check, strip 8th bit, and flow control
    raw.c_iflag &= ~(BRKINT | ICRNL | INPCK | ISTRIP | IXON);
    // Output flags: disable post-processing
    raw.c_oflag &= ~(OPOST);
    // Control flags: set character size to 8 bits
    raw.c_cflag |= (CS8);
    // Local flags: disable echoing, canonical mode, extended input processing, and signals
    raw.c_lflag &= ~(ECHO | ICANON | IEXTEN | ISIG);
    
    raw.c_cc[VMIN] = 0;
    raw.c_cc[VTIME] = 0;
    tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw);
}

void os_restore_terminal() {
    tcsetattr(STDIN_FILENO, TCSAFLUSH, &orig_termios);
}

int os_read_char() {
    char c;
    if (read(STDIN_FILENO, &c, 1) == 1) {
        return c;
    }
    return -1;
}

void os_get_terminal_size(int *width, int *height) {
    struct winsize w;
    if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &w) != -1) {
        *width = w.ws_col;
        *height = w.ws_row;
    } else {
        *width = 80;
        *height = 24;
    }
}
