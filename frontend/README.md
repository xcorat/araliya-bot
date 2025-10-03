# Araliya Bot Frontend

A sophisticated chat interface for the Araliya Graph-RAG AI assistant, built with Svelte 5, SvelteKit, and shadcn-svelte components.

## Features

- **Modern Chat Interface**: Clean, responsive design with message bubbles and real-time interactions
- **Session Management**: Create, switch, and manage multiple conversation sessions
- **Elegant Design System**: Botanical-inspired color palette with sophisticated typography
- **Accessibility First**: WCAG 2.1 AA compliant with full keyboard navigation
- **Mobile Responsive**: Seamless experience across all device sizes
- **Type Safe**: Full TypeScript integration for reliability

## Tech Stack

- **Framework**: Svelte 5 + SvelteKit
- **UI Components**: shadcn-svelte
- **Styling**: Tailwind CSS with custom design tokens
- **Icons**: Lucide Svelte
- **Package Manager**: pnpm
- **Build Tool**: Vite

## Getting Started

### Prerequisites

- Node.js 18+ 
- pnpm (recommended package manager)

### Installation

```bash
# Install dependencies
pnpm install

# Start development server
pnpm run dev

# Open in browser
pnpm run dev -- --open
```

### Available Scripts

```bash
# Development
pnpm run dev          # Start dev server
pnpm run dev -- --host # Start dev server accessible on network

# Building
pnpm run build        # Create production build
pnpm run preview      # Preview production build

# Quality Assurance
pnpm run check        # Run TypeScript checks
pnpm run lint         # Run ESLint
pnpm run format       # Format code with Prettier
pnpm run test         # Run unit tests
pnpm run test:e2e     # Run end-to-end tests
```

## Project Structure

```
src/
├── lib/
│   ├── components/
│   │   ├── ui/           # shadcn-svelte components
│   │   ├── chat/         # Chat-specific components
│   │   ├── layout/       # Layout components
│   │   └── session/      # Session management components
│   ├── stores/           # Svelte stores for state management
│   ├── api/              # API client and utilities
│   ├── utils/            # Utility functions
│   └── types/            # TypeScript type definitions
├── routes/               # SvelteKit routes
└── app.css              # Global styles and design tokens
```

## Design System

The application uses a sophisticated design system inspired by classic botanical illustrations:

### Color Palette
- **Primary**: Deep slate blue-grey (#3A4556)
- **Accent**: Muted teal (#5A9A9E) and soft rose (#D4A5A5)
- **Surface**: Off-white (#F8F9FA) and pure white (#FFFFFF)

### Typography
- **Primary**: Inter (UI elements)
- **Secondary**: Merriweather (emphasis)
- **Monospace**: JetBrains Mono (code)

## Backend Integration

The frontend is designed to integrate with the Phase 1 HF Space backend. Update the API base URL in `src/lib/api/client.ts`:

```typescript
const API_BASE_URL = 'https://your-hf-space-url.hf.space';
```

## Deployment

The application is configured for deployment to:
- **Cloudflare Pages** (recommended)
- **Vercel**
- **Netlify**

Build the application and deploy the `build/` directory to your chosen platform.

## Contributing

1. Follow the established code style (Prettier + ESLint)
2. Write tests for new functionality
3. Ensure accessibility standards are maintained
4. Use pnpm for package management

## License

Part of the Araliya Bot project.
