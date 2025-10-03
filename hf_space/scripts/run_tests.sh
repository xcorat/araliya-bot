#!/bin/bash
# Run tests using uv

cd "$(dirname "$0")/.."

echo "🧪 Running RAG tests with uv..."
echo ""

echo "1. Testing RAG service directly..."
uv run python scripts/test_rag.py

echo ""
echo "2. Testing API integration (requires server to be running)..."
echo "   Start server first: ./scripts/run_server.sh"
echo "   Then run: uv run python scripts/test_api_rag.py"
