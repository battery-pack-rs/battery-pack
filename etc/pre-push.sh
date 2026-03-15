#!/bin/sh

# Check for typos in the codebase using codespell
typos

if [ $? -ne 0 ]; then
    echo "You may use \`git push --no-verify\` to skip this check."
    exit 1
fi

cargo clippy -- -D clippy::all

if [ $? -ne 0 ]; then
    echo "You may use \`git push --no-verify\` to skip this check."
    exit 1
fi


cargo fmt --check

if [ $? -ne 0 ]; then
    echo "You may use \`git push --no-verify\` to skip this check."
    exit 1
fi
