#!/bin/bash

FILE="a.py"
echo "print('hello world')" > "$FILE"
KEYS_FULL="ciw\x1b"

echo "Running comparison for keys: $KEYS_FULL"
./tests/compare_with_nvim.sh "$FILE" "$KEYS_FULL"
RESULT=$?
rm "$FILE"
exit $RESULT
