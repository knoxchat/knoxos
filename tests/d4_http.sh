#!/bin/sh
# Tiny HTTP reply for QEMU user-net guestfwd (Gate D4).
printf 'HTTP/1.0 200 OK\r\nContent-Length: 16\r\n\r\nGATE_D4 http ok'
