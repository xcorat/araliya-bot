#!/bin/bash
# Update requirements.txt from pyproject.toml for HF Space compatibility

cd "$(dirname "$0")/.."

echo "📋 Updating requirements.txt for HF Space compatibility..."

# Generate requirements.txt from pyproject.toml
uv pip compile pyproject.toml -o requirements.txt

echo "✅ requirements.txt updated!"
echo ""
echo "Changes:"
git diff requirements.txt || echo "No git repository found"
