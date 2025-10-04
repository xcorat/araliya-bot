import { json } from '@sveltejs/kit';
import type { RequestEvent } from '@sveltejs/kit';

const HF_URL = import.meta.env.VITE_HF_URL || 'https://xcorat-araliya-bot.hf.space';
const HF_TOKEN = import.meta.env.VITE_HF_TOKEN;

export async function POST(event: RequestEvent) {
  try {
    const requestData = await event.request.json();
    
    console.log('HF_URL:', HF_URL);
    console.log('HF_TOKEN:', HF_TOKEN ? `${HF_TOKEN.substring(0, 10)}...` : 'undefined');
    console.log('Full URL:', `${HF_URL}/gradio_api/call/chat`);
    
    // Transform frontend format to Gradio format
    const gradioPayload = {
      data: [
        requestData.message,           // message string
        requestData.history || [],     // chat history array
        requestData.session_id || "default"  // session_id string
      ]
    };
    
    console.log('Gradio payload:', gradioPayload);
    
    // Forward the request to the HF Space Gradio API
    const response = await fetch(`${HF_URL}/gradio_api/call/chat`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(HF_TOKEN ? { 'Authorization': `Bearer ${HF_TOKEN}` } : {})
      },
      body: JSON.stringify(gradioPayload)
    });

    if (!response.ok) {
      const errorText = await response.text();
      console.error(`Error from HF Space: ${response.status} - ${errorText}`);
      return json({
        error: `Error from HF Space: ${response.status}`,
        message: errorText
      }, { status: response.status });
    }

    const gradioResponse = await response.json();
    console.log('Gradio response:', gradioResponse);
    
    // Transform Gradio response back to frontend format
    // Gradio returns: ["", [["user_message", "bot_response"]]]
    // We need: { message: { role: "assistant", content: "bot_response" }, session_id: "...", metadata: {...} }
    
    const [_, updatedHistory] = gradioResponse;
    const lastMessage = updatedHistory[updatedHistory.length - 1];
    const botResponse = lastMessage ? lastMessage[1] : "No response";
    
    const frontendResponse = {
      message: {
        role: "assistant",
        content: botResponse,
        timestamp: new Date().toISOString()
      },
      session_id: requestData.session_id || "default",
      metadata: {
        processingTime: 0 // Could calculate this if needed
      }
    };
    
    return json(frontendResponse);
  } catch (error) {
    console.error('Proxy error:', error);
    return json({
      error: 'Internal Server Error',
      message: error instanceof Error ? error.message : 'Unknown error'
    }, { status: 500 });
  }
};
