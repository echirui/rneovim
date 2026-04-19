#!/bin/bash

# Usage: ./compare_with_nvim.sh <filename> <keys_with_escapes>
# Example: ./compare_with_nvim.sh a.py "ciw\x1bux"

FILE=$1
KEYS_RAW=$2
RNEOVIM_OUT="actual.txt"
NVIM_OUT="expected.txt"

# Convert \x1b etc to actual bytes
KEYS_BYTES=$(printf "$KEYS_RAW")

# rneovim を実行
cargo run --release -- "$FILE" --test-keys "$KEYS_RAW" --test-output "$RNEOVIM_OUT"

# 本家 nvim を実行
# Use a temporary python script to drive nvim via RPC or just use --headless -c
# -c "normal! ..." works for many things, but ESC (\x1b) needs special handling.
# A more robust way is to use -s and ensure it works.
# Let's try sending keys via a temporary vim script that uses feedkeys
cat > run_test.vim <<EOF
set nocompatible
edit $FILE
execute "normal! :1G\r"
call feedkeys("$KEYS_RAW", 'tx')
write! $NVIM_OUT
quitall!
EOF

nvim --headless -u NONE -i NONE -S run_test.vim

# 比較
if diff "$RNEOVIM_OUT" "$NVIM_OUT"; then
    echo "SUCCESS: Results match!"
    rm "$RNEOVIM_OUT" "$NVIM_OUT" run_test.vim actual.txt expected.txt 2>/dev/null
    exit 0
else
    echo "FAILURE: Results differ."
    exit 1
fi
