#!/usr/bin/env bash
set -eu -o pipefail

mkfifo top-level

git init single-top-level-fifo
(cd single-top-level-fifo
  mkfifo top
)

git init two-fifos-two-files
(cd two-fifos-two-files
  mkdir dir dir-with-file
  touch file dir-with-file/nested-file

  mkfifo top
  mkfifo dir/nested
)
