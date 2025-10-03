# Simple Python base image compatible with HF Spaces
FROM python:3.11-slim

# Set environment variables
ENV PYTHONUNBUFFERED=1
ENV PYTHONDONTWRITEBYTECODE=1
ENV PIP_NO_CACHE_DIR=1
ENV PIP_DISABLE_PIP_VERSION_CHECK=1

# Set working directory
WORKDIR /code

# Copy requirements first for better Docker layer caching
COPY requirements.txt /code/requirements.txt

# Install Python dependencies with better error handling
RUN pip3 install --no-cache-dir --upgrade pip && \
    # First install numpy<2.0 for FAISS compatibility
    # See: https://github.com/facebookresearch/faiss/issues/3526
    # and https://github.com/huggingface/transformers/issues/31740
    pip3 install --no-cache-dir 'numpy<2.0' && \
    # Then install the rest of the requirements
    pip3 install --no-cache-dir --upgrade -r /code/requirements.txt

# Copy application code
COPY hf_space/app /code/app

# Create non-root user for security
RUN useradd -m -u 1000 user
USER user

# Set environment variables for HF Spaces compatibility
ENV HOME=/home/user \
    PATH=/home/user/.local/bin:$PATH

# Expose port 7860 (HF Spaces default)
EXPOSE 7860

# Health check
HEALTHCHECK --interval=30s --timeout=30s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:7860/api/v1/health || exit 1

# Start the application
CMD ["python3", "-m", "uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "7860"]
