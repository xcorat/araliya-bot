<script lang="ts">
	import { onMount } from 'svelte';
	import { Menu } from 'lucide-svelte';
	import { sessionsStore } from '$lib/stores/sessions.js';
	import { uiStore } from '$lib/stores/ui.js';
	import Sidebar from '$lib/components/layout/Sidebar.svelte';
	import MessageBubble from '$lib/components/chat/MessageBubble.svelte';
	import ChatInput from '$lib/components/chat/ChatInput.svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import type { ChatMessage, ChatSession } from '$lib/types/chat.js';
	import { generateId } from '$lib/utils/helpers.js';
	import { apiClient } from '$lib/api/client.js';

	let sessions = $state<ChatSession[]>([]);
	let ui = $state<{
		sidebarOpen: boolean;
		loading: boolean;
		error: string | null;
		activeSessionId: string | null;
	}>({
		sidebarOpen: false,
		loading: false,
		error: null,
		activeSessionId: null
	});

	let currentSession = $derived(
		sessions.find(s => s.id === ui.activeSessionId)
	);

	onMount(() => {
		// Subscribe to stores
		sessionsStore.subscribe(value => sessions = value);
		uiStore.subscribe(value => ui = value);

		// Create initial session if none exist
		if (sessions.length === 0) {
			const sessionId = sessionsStore.createSession();
			uiStore.setActiveSession(sessionId);
		} else {
			// Set first session as active if none selected
			if (!ui.activeSessionId && sessions.length > 0) {
				uiStore.setActiveSession(sessions[0].id);
			}
		}
	});

	function handleNewChat() {
		const sessionId = sessionsStore.createSession();
		uiStore.setActiveSession(sessionId);
		uiStore.setSidebarOpen(false);
	}

	function handleSessionSelect(sessionId: string) {
		uiStore.setActiveSession(sessionId);
		uiStore.setSidebarOpen(false);
	}

	function handleSessionDelete(sessionId: string) {
		sessionsStore.deleteSession(sessionId);
		
		// If deleted session was active, switch to another
		if (ui.activeSessionId === sessionId) {
			const remainingSessions = sessions.filter(s => s.id !== sessionId);
			if (remainingSessions.length > 0) {
				uiStore.setActiveSession(remainingSessions[0].id);
			} else {
				// Create new session if none left
				const newSessionId = sessionsStore.createSession();
				uiStore.setActiveSession(newSessionId);
			}
		}
	}

	async function handleSendMessage(content: string) {
		if (!ui.activeSessionId) return;

		uiStore.setLoading(true);
		uiStore.clearError();

		try {
			// Add user message
			const userMessage: ChatMessage = {
				id: generateId(),
				content,
				role: 'user',
				timestamp: new Date()
			};

			// Update session with user message
			const session = sessionsStore.getSession(ui.activeSessionId);
			if (session) {
				session.messages.push(userMessage);
				sessionsStore.updateSession(ui.activeSessionId, {
					messages: session.messages,
					messageCount: session.messages.length,
					lastMessage: content
				});

				// Update title if this is the first message
				if (session.messages.length === 1) {
					sessionsStore.updateSessionTitle(ui.activeSessionId, content);
				}
			}

			// Call the API to get AI response
			try {
				const response = await apiClient.sendMessage({
					message: content,
					session_id: ui.activeSessionId,
					history: session?.messages || []
				});

				// Create AI message from response
				const aiMessage: ChatMessage = {
					id: generateId(),
					content: response.message,
					role: 'assistant',
					timestamp: new Date(),
					metadata: {
						processingTime: response.metadata.processingTime,
						tokens: response.metadata.tokenUsage?.total_tokens || 0,
						model: response.metadata.model
					}
				};

				const updatedSession = sessionsStore.getSession(ui.activeSessionId!);
				if (updatedSession) {
					updatedSession.messages.push(aiMessage);
					sessionsStore.updateSession(ui.activeSessionId!, {
						messages: updatedSession.messages,
						messageCount: updatedSession.messages.length,
						lastMessage: aiMessage.content
					});
				}
			} catch (apiError) {
				console.error('API Error:', apiError);
				uiStore.setError('Failed to get response from AI. Please try again.');
			}

			uiStore.setLoading(false);

		} catch (error) {
			console.error('Failed to send message:', error);
			uiStore.setError('Failed to send message. Please try again.');
			uiStore.setLoading(false);
		}
	}

	function handleCopyMessage(content: string) {
		// TODO: Show toast notification
		console.log('Message copied:', content);
	}
</script>

<svelte:head>
	<title>Araliya Bot - AI Assistant</title>
	<meta name="description" content="Chat with Araliya Bot, your intelligent AI assistant" />
</svelte:head>

<div class="flex h-screen overflow-hidden">
	<!-- Sidebar -->
	<Sidebar
		{sessions}
		activeSessionId={ui.activeSessionId}
		isOpen={ui.sidebarOpen}
		onNewChat={handleNewChat}
		onSessionSelect={handleSessionSelect}
		onSessionDelete={handleSessionDelete}
		onClose={() => uiStore.setSidebarOpen(false)}
	/>

	<!-- Main Content -->
	<div class="flex-1 flex flex-col">
		<!-- Header -->
		<header class="flex items-center justify-between p-4 border-b border-decorative-border bg-surface-elevated">
			<div class="flex items-center gap-3">
				<Button
					variant="ghost"
					size="sm"
					onclick={() => uiStore.toggleSidebar()}
					class="md:hidden h-8 w-8 p-0"
					aria-label="Toggle sidebar"
				>
					<Menu class="w-4 h-4" />
				</Button>
				
				<h1 class="text-heading font-semibold text-text-primary">
					{currentSession?.title || 'New Chat'}
				</h1>
			</div>

			{#if ui.error}
				<div class="text-body-small text-semantic-error">
					{ui.error}
				</div>
			{/if}
		</header>

		<!-- Messages -->
		<main class="flex-1 overflow-y-auto p-4">
			<div class="max-w-4xl mx-auto space-y-4">
				{#if currentSession?.messages.length === 0}
					<!-- Empty state -->
					<div class="flex flex-col items-center justify-center h-64 text-center">
						<div class="w-16 h-16 bg-accent-primary/10 rounded-full flex items-center justify-center mb-4">
							<div class="w-8 h-8 bg-accent-primary rounded-full"></div>
						</div>
						<h2 class="text-heading text-text-primary mb-2">Welcome to Araliya Bot</h2>
						<p class="text-body text-text-secondary max-w-md">
							Start a conversation by typing a message below. I'm here to help with your questions and tasks.
						</p>
					</div>
				{:else}
					<!-- Messages -->
					{#each currentSession?.messages || [] as message (message.id)}
						<MessageBubble
							{message}
							onCopy={handleCopyMessage}
						/>
					{/each}

					<!-- Loading indicator -->
					{#if ui.loading}
						<div class="flex justify-start">
							<div class="flex items-center gap-3">
								<div class="w-8 h-8 rounded-full bg-accent-primary flex items-center justify-center">
									<div class="w-4 h-4 bg-white rounded-full"></div>
								</div>
								<div class="bg-surface-elevated border border-decorative-border rounded-lg rounded-bl-sm px-4 py-3">
									<div class="flex gap-1">
										<div class="w-2 h-2 bg-accent-primary rounded-full animate-pulse"></div>
										<div class="w-2 h-2 bg-accent-primary rounded-full animate-pulse" style="animation-delay: 0.2s"></div>
										<div class="w-2 h-2 bg-accent-primary rounded-full animate-pulse" style="animation-delay: 0.4s"></div>
									</div>
								</div>
							</div>
						</div>
					{/if}
				{/if}
			</div>
		</main>

		<!-- Chat Input -->
		<ChatInput
			onSend={handleSendMessage}
			disabled={ui.loading}
			autoFocus={true}
		/>
	</div>
</div>
