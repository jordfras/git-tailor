#!/usr/bin/env bash
# Build a scratch repo for driving gt: `main` holds a base commit, `feature`
# the commits to work on, so `gt main` lists exactly those.
#
#   fixture.sh DIR          "change" edits text in a.txt (2 hunks), b.txt and
#                           both.sh, modifies binary bin.dat, adds an empty
#                           file, chmod +x run.sh (mode only) and both.sh;
#                           "later" touches b.txt again, so there are two
#                           hunk groups
#   fixture.sh DIR single   one commit: a single text hunk beside a binary file
#
# DIR is deleted first.
set -euo pipefail
rm -rf "$1"; mkdir -p "$1"; cd "$1"
git init -q -b main
git config user.name "Test User"; git config user.email test@example.com

if [ "${2:-}" = single ]; then
    printf 'a1\n' > a.txt; printf '\0old\0' > bin.dat
    git add -A; git commit -qm base; git switch -qc feature
    printf 'a2\n' > a.txt; printf '\0new\0' > bin.dat
    git add -A; git commit -qm "one hunk beside binary"
    exit
fi

seq 1 12 > a.txt; echo b1 > b.txt; printf '\0old\0' > bin.dat
echo run > run.sh; echo x > both.sh
git add -A; git commit -qm base; git switch -qc feature
sed -i 's/^1$/1X/; s/^12$/12X/' a.txt; echo b2 > b.txt; printf '\0new\0' > bin.dat
: > empty.txt; chmod +x run.sh both.sh; echo y > both.sh
git add -A; git commit -qm change
echo b3 > b.txt; git commit -qam later
