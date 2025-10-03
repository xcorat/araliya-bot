import { writable } from 'svelte/store';
import { browser } from '$app/environment';

interface UIState {
  sidebarOpen: boolean;
  loading: boolean;
  error: string | null;
  activeSessionId: string | null;
  theme: 'light' | 'dark';
}

const STORAGE_KEY = 'araliya_ui_state';

function createUIStore() {
  const initialState: UIState = {
    sidebarOpen: false,
    loading: false,
    error: null,
    activeSessionId: null,
    theme: 'light'
  };

  // Load persisted state
  if (browser) {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        Object.assign(initialState, {
          theme: parsed.theme || 'light',
          activeSessionId: parsed.activeSessionId || null
        });
      } catch (error) {
        console.warn('Failed to parse saved UI state:', error);
      }
    }
  }

  const { subscribe, set, update } = writable<UIState>(initialState);

  // Persist certain state changes
  if (browser) {
    subscribe((state) => {
      const persistedState = {
        theme: state.theme,
        activeSessionId: state.activeSessionId
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(persistedState));
    });
  }

  return {
    subscribe,
    
    toggleSidebar() {
      update(state => ({ ...state, sidebarOpen: !state.sidebarOpen }));
    },

    setSidebarOpen(open: boolean) {
      update(state => ({ ...state, sidebarOpen: open }));
    },

    setLoading(loading: boolean) {
      update(state => ({ ...state, loading }));
    },

    setError(error: string | null) {
      update(state => ({ ...state, error }));
    },

    clearError() {
      update(state => ({ ...state, error: null }));
    },

    setActiveSession(sessionId: string | null) {
      update(state => ({ ...state, activeSessionId: sessionId }));
    },

    setTheme(theme: 'light' | 'dark') {
      update(state => ({ ...state, theme }));
    },

    reset() {
      set(initialState);
    }
  };
}

export const uiStore = createUIStore();
