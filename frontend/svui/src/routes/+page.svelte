<script lang="ts">
    import { onMount, onDestroy } from "svelte";
    import { RefreshCw } from "@lucide/svelte";
    import { ChatMessages } from "$lib/components/chat";
    import UniwebInput from "./UniwebInput.svelte";
    import {
        initBaseUrl,
        doCheckHealth,
        getMessages,
        getSessionId,
        getIsLoading,
        getIsHistoryLoading,
        initSharedSession,
        refreshSharedMessages,
    } from "$lib/state.svelte";

    const POLL_INTERVAL_MS = 30_000;
    let pollTimer: ReturnType<typeof setInterval> | null = null;

    function startPolling() {
        stopPolling();
        pollTimer = setInterval(async () => {
            if (document.visibilityState === "visible") {
                await refreshSharedMessages();
            }
        }, POLL_INTERVAL_MS);
    }

    function stopPolling() {
        if (pollTimer !== null) {
            clearInterval(pollTimer);
            pollTimer = null;
        }
    }

    onMount(async () => {
        initBaseUrl();
        doCheckHealth();
        // Load the shared transcript so all visitors see the same conversation.
        // TODO: CHECK: uniweb should be init only if its ON, and still maybe better done elsewhere?
        await initSharedSession("uniweb");
        startPolling();
    });

    onDestroy(() => {
        stopPolling();
    });

    async function handleRefresh() {
        await refreshSharedMessages();
    }

    const messages = $derived(getMessages());
    const sessionId = $derived(getSessionId());
    const isLoading = $derived(getIsLoading());
    const isHistoryLoading = $derived(getIsHistoryLoading());
</script>

<svelte:head>
    <title>Araliya — Uniweb</title>
</svelte:head>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
    <header class="flex items-center justify-between border-b px-4 py-2">
        <div class="flex items-center gap-2">
            <h1 class="text-lg font-semibold">Front Porch</h1>
            <span class="text-xs text-muted-foreground"
                >shared session — everyone sees the same conversation</span
            >
        </div>
        <div class="flex items-center gap-2">
            {#if isLoading}
                <span class="text-xs text-muted-foreground animate-pulse"
                    >processing…</span
                >
            {/if}
            <button
                onclick={handleRefresh}
                disabled={isHistoryLoading || isLoading}
                title="Refresh messages"
                class="rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40"
            >
                <RefreshCw
                    class="h-4 w-4 {isHistoryLoading ? 'animate-spin' : ''}"
                />
            </button>
        </div>
    </header>

    <ChatMessages {messages} />
    <UniwebInput />
</div>
