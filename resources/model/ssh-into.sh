#!/usr/bin/env bash

if [ -z "$1" ]; then
  echo 'host expected!'
  exit 1
fi

if [ $1 == 'zeus' ]; then
    PORT=8820
fi
if [ $1 == 'ares' ]; then
    PORT=8821
fi

ssh root@127.0.0.1 -p $PORT -i ./keys/vagrant