<script lang="ts">
    // TODO: CHECK:  what the f is this file?
    import Button from "$lib/components/ui/button/button.svelte";
    import { getIsLoading, doSendMessageStreaming } from "$lib/state.svelte";
    import { SendHorizontal, Loader2 } from "@lucide/svelte";

    const AGENT_ID = "uniweb";

    let inputText = $state("");
    let textareaEl = $state<HTMLTextAreaElement | null>(null);

    const loading = $derived(getIsLoading());
    const canSend = $derived(inputText.trim().length > 0 && !loading);

    function autoResize() {
        if (textareaEl) {
            textareaEl.style.height = "auto";
            textareaEl.style.height =
                Math.min(textareaEl.scrollHeight, 200) + "px";
        }
    }

    async function handleSend() {
        if (!canSend) return;
        const text = inputText;
        inputText = "";
        if (textareaEl) {
            textareaEl.style.height = "auto";
        }
        await doSendMessageStreaming(text, AGENT_ID);
        textareaEl?.focus();
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            handleSend();
        }
    }
</script>

<div class="border-t bg-background/80 px-4 py-3 backdrop-blur-sm">
    <div class="mx-auto flex max-w-3xl items-end gap-2">
        <textarea
            bind:this={textareaEl}
            bind:value={inputText}
            oninput={autoResize}
            onkeydown={handleKeydown}
            placeholder="Type a message… (shared with everyone)"
            disabled={loading}
            rows={1}
            class="flex-1 resize-none rounded-xl border bg-muted/50 px-4 py-2.5 text-sm outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-ring focus:ring-1 focus:ring-ring disabled:opacity-50 {loading
                ? 'opacity-60'
                : ''}"
        ></textarea>
        <Button
            size="icon"
            disabled={!canSend}
            onclick={handleSend}
            class="shrink-0 rounded-xl transition-opacity"
        >
            {#if loading}
                <Loader2 class="h-4 w-4 animate-spin" />
            {:else}
                <SendHorizontal class="h-4 w-4" />
            {/if}
        </Button>
    </div>
    {#if loading}
        <p class="mx-auto mt-1 max-w-3xl text-xs text-muted-foreground">
            Processing… other messages will wait in queue.
        </p>
    {/if}
</div>
