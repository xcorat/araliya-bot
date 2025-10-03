"""
Araliya Bot - Entry point for Hugging Face Spaces
This file imports and exposes the FastAPI app from the hf_space module.
"""

import sys
import os

# Add the project root to the Python path
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

# Import the app from hf_space
from hf_space.app.main import app

# This file is required for compatibility with Hugging Face Spaces
# The actual implementation is in the hf_space/app directory

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=7860)
