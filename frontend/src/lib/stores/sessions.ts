import { writable } from 'svelte/store';
import type { ChatSession } from '$lib/types/chat.js';
import { generateId, truncateText } from '$lib/utils/helpers.js';
import { browser } from '$app/environment';

const STORAGE_KEY = 'araliya_sessions';
const MAX_SESSIONS = 50;

function createSessionStore() {
  // Load initial data from localStorage
  const initialSessions: ChatSession[] = browser 
    ? JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]').map((s: any) => ({
        ...s,
        createdAt: new Date(s.createdAt),
        updatedAt: new Date(s.updatedAt)
      }))
    : [];

  const { subscribe, set, update } = writable<ChatSession[]>(initialSessions);

  // Save to localStorage whenever sessions change
  if (browser) {
    subscribe((sessions) => {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(sessions));
    });
  }

  return {
    subscribe,
    
    createSession(): string {
      const newSession: ChatSession = {
        id: generateId(),
        title: 'New Chat',
        createdAt: new Date(),
        updatedAt: new Date(),
        messageCount: 0,
        messages: []
      };

      update(sessions => {
        const updatedSessions = [newSession, ...sessions];
        // Keep only the most recent sessions
        return updatedSessions.slice(0, MAX_SESSIONS);
      });

      return newSession.id;
    },

    updateSession(sessionId: string, updates: Partial<ChatSession>) {
      update(sessions => 
        sessions.map(session => 
          session.id === sessionId 
            ? { ...session, ...updates, updatedAt: new Date() }
            : session
        )
      );
    },

    updateSessionTitle(sessionId: string, firstMessage: string) {
      const title = truncateText(firstMessage, 50) || 'New Chat';
      this.updateSession(sessionId, { title });
    },

    deleteSession(sessionId: string) {
      update(sessions => sessions.filter(session => session.id !== sessionId));
    },

    getSession(sessionId: string): ChatSession | undefined {
      let foundSession: ChatSession | undefined;
      subscribe(sessions => {
        foundSession = sessions.find(session => session.id === sessionId);
      })();
      return foundSession;
    },

    clear() {
      set([]);
      if (browser) {
        localStorage.removeItem(STORAGE_KEY);
      }
    }
  };
}

export const sessionsStore = createSessionStore();
