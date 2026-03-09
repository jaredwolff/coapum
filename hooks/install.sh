#!/bin/bash

# Install pre-commit hooks for Rust development
# Run this script after cloning the repository: ./hooks/install.sh

HOOKS_DIR="$(cd "$(dirname "$0")" && pwd)"
GIT_HOOKS_DIR="$(git rev-parse --git-dir)/hooks"

echo "Installing pre-commit hooks..."

# Copy hooks and make them executable
cp "$HOOKS_DIR/pre-commit" "$GIT_HOOKS_DIR/pre-commit"
chmod +x "$GIT_HOOKS_DIR/pre-commit"

echo "✅ Pre-commit hooks installed successfully!"
echo ""
echo "The hook will run before each commit:"
echo "  • cargo clippy (linting)"
echo "  • cargo fmt --check (formatting)"
echo "(Tests are optional in the hook; run locally with 'cargo test')"
echo ""
echo "To skip the hook temporarily: git commit --no-verify"
