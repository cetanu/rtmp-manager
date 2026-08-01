set shell := ["bash", "-c"]

default:
    @just --list

build:
    cargo build

check:
    cargo check

run:
    cargo run

test-stream ip key="teststream":
    ffmpeg -re -f lavfi -i testsrc=size=1280x720:rate=30 -f lavfi -i sine=frequency=1000 -c:v libx264 -c:a aac -f flv "rtmp://{{ip}}:1935/live/{{key}}"

test-file ip file key="teststream":
    ffmpeg -re -i "{{file}}" -c copy -f flv "rtmp://{{ip}}:1935/live/{{key}}"
