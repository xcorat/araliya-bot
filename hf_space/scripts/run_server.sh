#!/bin/bash
# Run the development server using uv

cd "$(dirname "$0")/.."

echo "🚀 Starting Araliya Bot server with uv..."
uv run python app/main.py
