#!/bin/bash

# Target file
FILE="a.py"
echo "print('hello world')" > "$FILE"

# Sequence of keys: ciw (change inner word), ESC, u (undo), x (delete char)
# Note: In our test implementation, \x1b is used for ESC.
KEYS="ciw\x1bu"
# Wait, the user asked for: c i w ESC u x
# So keys are: ciw, \x1b, u, x
KEYS_FULL="ciw\x1bux"

echo "Running comparison for keys: $KEYS_FULL on file content: $(cat $FILE)"

./tests/compare_with_nvim.sh "$FILE" "$KEYS_FULL"

RESULT=$?

# Cleanup
rm "$FILE"

exit $RESULT
