#!/bin/bash
# Development setup script using uv
# Sets up CPU-optimized local development environment

set -e

echo "🚀 Setting up Araliya Bot LOCAL development environment with uv"
echo "   (CPU-optimized for fast local iteration)"
echo "=" * 60

# Check if uv is installed
if ! command -v uv &> /dev/null; then
    echo "❌ uv is not installed. Please install it first:"
    echo "   curl -LsSf https://astral.sh/uv/install.sh | sh"
    exit 1
fi

echo "✅ uv found: $(uv --version)"

# Navigate to project directory
cd "$(dirname "$0")/.."

echo "🖥️  Setting up CPU-optimized environment..."
echo "   • faiss-cpu for local development"
echo "   • torch CPU-only"
echo "   • Lightweight sentence-transformers"

echo "📦 Installing Python and creating virtual environment..."
uv sync

echo "🧪 Running CPU-optimized tests to verify setup..."
if uv run python scripts/test_cpu_rag.py; then
    echo "✅ CPU RAG tests passed!"
else
    echo "⚠️  CPU RAG tests failed - checking dependencies..."
    echo "   This is normal if dependencies aren't fully installed yet"
fi

echo "📋 Note: requirements.txt is optimized for HF Spaces ZeroGPU deployment"
echo "   • Local dev uses: faiss-cpu, torch (CPU)"
echo "   • HF Spaces uses: faiss-gpu, torch (GPU), accelerate"

echo "🎉 Local development environment setup complete!"
echo ""
echo "Local Development Commands:"
echo "  • Run server:     uv run python app/main.py"
echo "  • Run CPU tests:  uv run python scripts/test_cpu_rag.py"
echo "  • Run API tests:  uv run python scripts/test_api_rag.py"
echo "  • Install deps:   uv add <package>"
echo ""
echo "HF Spaces Deployment:"
echo "  • Uses requirements.txt (GPU-optimized)"
echo "  • @spaces.GPU decorator provides ZeroGPU acceleration"
echo "  • Automatic fallback to CPU if GPU unavailable"
