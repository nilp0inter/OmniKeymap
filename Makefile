SHELL := /bin/sh

# OmniKeymap build automation.
# Targets: build, test, lint, extract, book, clean.

CARGO ?= cargo
NIX_RUN ?= nix run nixpkgs#

.PHONY: all build test lint fmt check extract-linux extract-all book book-serve clean

all: build

# Build the workspace.
build:
	$(CARGO) build --workspace

# Build with parallel jobs (override JOBS).
build-j:
	$(CARGO) build --workspace -j $(JOBS)

# Run all workspace tests.
test:
	$(CARGO) test --workspace

# Lint with clippy.
lint:
	$(CARGO) clippy --workspace -- -D warnings

# Format check / apply.
fmt:
	$(CARGO) fmt --all

check:
	$(CARGO) check --workspace

# Extract the current Linux XKB layout (override LAYOUT/VARIANT).
LAYOUT ?= us
VARIANT ?=
EXTRACT_DIR ?= database/linux
extract-linux:
	$(CARGO) run -p omni-keymap-extract -- \
	    --platform linux --layout $(LAYOUT) \
	    $(if $(VARIANT),--layout-variant $(VARIANT)) \
	    --out-dir $(EXTRACT_DIR)

# Extract every XKB layout/variant from evdev.lst into database/linux/.
# On NixOS: make extract-all XKB_CONFIG_ROOT=$(nix path-info nixpkgs#xkeyboard_config)/share/xkeyboard-config-2
extract-all:
	$(CARGO) run -p omni-keymap-extract -- --platform linux --all --out-dir database/linux

# Build the mdBook documentation under docs/.
book:
	$(NIX_RUN)mdbook -- build docs

# Serve the mdBook documentation locally on http://127.0.0.1:3000.
book-serve:
	$(NIX_RUN)mdbook -- serve docs

# Clean build artifacts and the generated book.
clean:
	$(CARGO) clean
	rm -rf docs/book