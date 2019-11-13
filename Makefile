SHELL:=/bin/bash

.DEFAULT_GOAL := default

format:
	cargo fmt

clippy:
	cargo clippy

build: format clippy
	cargo build

install: format
	cargo install --force --path .

book: install
	mdbook build

default: build

clean:
	cargo clean