# Hybrid Development Setup

This document explains the hybrid setup for **CPU local development** with `uv` and **ZeroGPU deployment** on HF Spaces.

## Architecture Overview

```
Local Development (CPU)          HF Spaces Deployment (ZeroGPU)
├── uv package manager          ├── requirements.txt
├── pyproject.toml              ├── @spaces.GPU decorators  
├── faiss-cpu                   ├── faiss-gpu
├── torch (CPU-only)            ├── torch (GPU-accelerated)
└── Fast iteration              └── High performance
```

## Benefits

### Local Development (CPU + uv)
- ⚡ **Fast setup**: `uv sync` installs dependencies quickly
- 💻 **CPU-optimized**: Works on any machine without GPU
- 🔄 **Quick iteration**: Fast startup and testing
- 📦 **Modern tooling**: pyproject.toml, lockfiles, dev dependencies

### HF Spaces Deployment (ZeroGPU)
- 🚀 **GPU acceleration**: ZeroGPU for embedding generation
- ⚡ **High performance**: faiss-gpu for fast similarity search
- 🔧 **Auto-scaling**: Spaces handles GPU allocation
- 🎯 **Production ready**: Optimized for user-facing deployment

## Setup Instructions

### 1. Local Development Setup

```bash
# Install uv (if not already installed)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Setup local environment
cd hf_space
./scripts/dev_setup.sh
```

This creates:
- CPU-optimized virtual environment
- faiss-cpu for local vector search
- All dependencies for local development

### 2. Local Development Commands

```bash
# Run development server
uv run python app/main.py

# Run CPU-optimized tests
uv run python scripts/test_cpu_rag.py

# Run API integration tests
uv run python scripts/test_api_rag.py

# Add new dependencies
uv add <package-name>

# Update lockfile
uv lock
```

### 3. HF Spaces Deployment

The `requirements.txt` file is optimized for HF Spaces ZeroGPU:

```txt
# GPU-optimized packages for HF Spaces ZeroGPU
faiss-gpu
sentence-transformers
torch
transformers
accelerate
spaces
```

## Smart Environment Detection

The RAG service automatically detects the environment:

```python
# Smart FAISS import: GPU for HF Spaces, CPU for local
if os.environ.get('SPACE_ID') or os.environ.get('SPACES_ZERO_GPU'):
    # HF Spaces - use GPU acceleration
    FAISS_GPU_AVAILABLE = True
else:
    # Local development - use CPU
    FAISS_GPU_AVAILABLE = False
```

## File Structure

```
hf_space/
├── pyproject.toml              # Local dev dependencies (CPU)
├── requirements.txt            # HF Spaces dependencies (GPU)
├── .python-version            # Python version for uv
├── app/
│   ├── services/
│   │   └── rag_service.py     # Smart CPU/GPU detection
│   └── main.py                # @spaces.GPU decorators
└── scripts/
    ├── dev_setup.sh           # Local setup with uv
    ├── test_cpu_rag.py        # CPU-optimized tests
    └── run_server.sh          # Local development server
```

## Dependency Management

### Local Development (pyproject.toml)
```toml
dependencies = [
    "faiss-cpu",           # CPU vector search
    "torch",               # CPU-only PyTorch
    "sentence-transformers",
    # ... other deps
]
```

### HF Spaces (requirements.txt)
```txt
faiss-gpu              # GPU vector search
torch                  # GPU-accelerated PyTorch  
accelerate             # GPU optimization
spaces                 # ZeroGPU decorator
```

## Performance Comparison

| Aspect | Local CPU | HF Spaces ZeroGPU |
|--------|-----------|-------------------|
| **Setup Time** | ~30s with uv | ~2-3min first deploy |
| **Embedding Speed** | ~1-2s per query | ~0.1-0.3s per query |
| **Vector Search** | ~10-50ms | ~1-5ms |
| **Memory Usage** | ~500MB-1GB | ~2-4GB |
| **Cost** | Free | Free (ZeroGPU quota) |

## Best Practices

### Local Development
1. Use `uv run` for all commands
2. Test with `test_cpu_rag.py` for quick validation
3. Keep dependencies minimal in pyproject.toml
4. Use CPU-friendly batch sizes for testing

### HF Spaces Deployment
1. Use `@spaces.GPU` for compute-intensive functions
2. Optimize for GPU batch processing
3. Handle GPU/CPU fallback gracefully
4. Monitor ZeroGPU usage quotas

## Troubleshooting

### Local Issues
```bash
# Reinstall dependencies
uv sync --reinstall

# Check Python version
uv python list

# Verify CPU setup
uv run python scripts/test_cpu_rag.py
```

### HF Spaces Issues
- Check GPU availability in logs
- Verify @spaces.GPU decorators are applied
- Monitor memory usage (GPU has limits)
- Test fallback to CPU mode

## Migration Commands

### Add New Dependency
```bash
# Add to local development
uv add <package>

# Manually add to requirements.txt for HF Spaces
echo "<package>" >> requirements.txt
```

### Update Both Environments
```bash
# Update local
uv add <package>

# Update HF Spaces requirements
# (manually sync pyproject.toml -> requirements.txt)
```

This hybrid setup gives you the best of both worlds: fast local iteration and powerful cloud deployment! 🚀
