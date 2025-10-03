"""
Sample blog post data for RAG testing.
This represents the kind of content that would be ingested from actual blog sources.
"""

SAMPLE_BLOG_POSTS = [
    {
        "id": "post_1",
        "title": "Introduction to Graph Neural Networks",
        "content": """Graph Neural Networks (GNNs) are a powerful class of deep learning models designed to work with graph-structured data. Unlike traditional neural networks that operate on Euclidean data like images or text sequences, GNNs can handle non-Euclidean data where relationships between entities are explicitly modeled as edges in a graph.

The key innovation of GNNs lies in their ability to perform message passing between nodes, allowing information to flow through the graph structure. This makes them particularly effective for tasks like node classification, link prediction, and graph-level predictions.

Popular GNN architectures include Graph Convolutional Networks (GCNs), GraphSAGE, and Graph Attention Networks (GATs). Each has its own approach to aggregating information from neighboring nodes.""",
        "author": "AI Research Blog",
        "date": "2024-01-15",
        "tags": ["machine-learning", "graph-neural-networks", "deep-learning"]
    },
    {
        "id": "post_2", 
        "title": "Retrieval-Augmented Generation Explained",
        "content": """Retrieval-Augmented Generation (RAG) combines the power of large language models with external knowledge retrieval. Instead of relying solely on the model's training data, RAG systems can access and incorporate relevant information from external databases or document collections.

The RAG process typically involves three steps: 1) Retrieving relevant documents based on the input query, 2) Encoding both the query and retrieved documents, and 3) Generating a response that incorporates the retrieved information.

This approach addresses some key limitations of pure language models, including hallucination and outdated information. RAG systems can provide more accurate, up-to-date, and grounded responses by leveraging external knowledge sources.""",
        "author": "ML Engineering Blog",
        "date": "2024-02-10",
        "tags": ["rag", "llm", "information-retrieval"]
    },
    {
        "id": "post_3",
        "title": "Vector Databases and Semantic Search",
        "content": """Vector databases have emerged as a crucial component in modern AI applications, particularly for semantic search and recommendation systems. These databases store high-dimensional vector representations of data, enabling similarity-based queries that go beyond traditional keyword matching.

Popular vector databases include Pinecone, Weaviate, Chroma, and FAISS. Each offers different trade-offs in terms of performance, scalability, and ease of use. FAISS, developed by Facebook AI Research, is particularly popular for its efficiency and flexibility.

The key advantage of vector databases is their ability to perform semantic similarity search. Instead of matching exact keywords, they can find conceptually similar content based on the vector representations learned by embedding models.""",
        "author": "Data Engineering Weekly",
        "date": "2024-02-20",
        "tags": ["vector-database", "semantic-search", "embeddings"]
    },
    {
        "id": "post_4",
        "title": "Building Chatbots with FastAPI",
        "content": """FastAPI has become a popular choice for building high-performance APIs, including chatbot backends. Its automatic API documentation, type hints, and async support make it ideal for real-time applications.

When building a chatbot with FastAPI, key considerations include session management, rate limiting, and error handling. The framework's dependency injection system makes it easy to manage shared resources like database connections and AI model instances.

For production deployments, FastAPI applications can be easily containerized and deployed on platforms like Hugging Face Spaces, which provides free GPU access for AI applications.""",
        "author": "Python Web Development",
        "date": "2024-03-01",
        "tags": ["fastapi", "chatbot", "python", "api"]
    },
    {
        "id": "post_5",
        "title": "Svelte vs React: Modern Frontend Frameworks",
        "content": """Svelte has gained significant traction as an alternative to React for building user interfaces. Unlike React, Svelte is a compile-time framework that generates vanilla JavaScript, resulting in smaller bundle sizes and better performance.

Key advantages of Svelte include its simple syntax, built-in state management, and excellent developer experience. SvelteKit, the full-stack framework built on Svelte, provides features like server-side rendering, routing, and API endpoints.

For chat applications, Svelte's reactive nature makes it easy to handle real-time updates and manage conversation state. The framework's small footprint is particularly beneficial for mobile users.""",
        "author": "Frontend Focus",
        "date": "2024-03-15",
        "tags": ["svelte", "react", "frontend", "javascript"]
    }
]
