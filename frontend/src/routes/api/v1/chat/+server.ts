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
    // Gradio expects: [message, history, session_id]
    const gradioPayload = {
      data: [
        requestData.message,           // message string
        [],                           // empty history for now
        requestData.session_id || "default"  // session_id string
      ]
    };
    
    console.log('Gradio payload:', gradioPayload);
    
    // Step 1: Submit request to get event_id
    const response = await fetch(`${HF_URL}/gradio_api/call/chat`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(HF_TOKEN ? { 'Authorization': `Bearer ${HF_TOKEN}` } : {})
      },
      body: JSON.stringify(gradioPayload)
    });
    console.log('Response:', response);

    if (!response.ok) {
      const errorText = await response.text();
      console.error(`Error from HF Space: ${response.status} - ${errorText}`);
      return json({
        error: `Error from HF Space: ${response.status}`,
        message: errorText
      }, { status: response.status });
    }

    const eventResponse = await response.json();
    console.log('Event response:', eventResponse);
    
    if (!eventResponse.event_id) {
      return json({
        error: 'No event_id received from Gradio',
        message: 'Invalid response format'
      }, { status: 500 });
    }

    // Step 2: Stream the result using event_id
    const resultResponse = await fetch(`${HF_URL}/gradio_api/call/chat/${eventResponse.event_id}`, {
      method: 'GET',
      headers: {
        ...(HF_TOKEN ? { 'Authorization': `Bearer ${HF_TOKEN}` } : {})
      }
    });

    if (!resultResponse.ok) {
      const errorText = await resultResponse.text();
      console.error(`Error getting result: ${resultResponse.status} - ${errorText}`);
      return json({
        error: `Error getting result: ${resultResponse.status}`,
        message: errorText
      }, { status: resultResponse.status });
    }

    // Parse the Server-Sent Events stream
    const resultText = await resultResponse.text();
    console.log('Result stream:', resultText);
    
    // Extract the final result from the event stream
    const lines = resultText.split('\n');
    let finalData = null;
    
    for (const line of lines) {
      if (line.startsWith('event: complete')) {
        // Find the corresponding data line
        const dataLineIndex = lines.indexOf(line) + 1;
        if (dataLineIndex < lines.length && lines[dataLineIndex].startsWith('data: ')) {
          try {
            finalData = JSON.parse(lines[dataLineIndex].substring(6)); // Remove 'data: '
            break;
          } catch (e) {
            console.error('Error parsing final data:', e);
          }
        }
      }
    }
    
    if (!finalData) {
      return json({
        error: 'No complete event found in stream',
        message: 'Failed to get final result'
      }, { status: 500 });
    }
    
    // Transform Gradio response back to frontend format
    // Gradio returns: [null, [["user_message", "bot_response"]]]
    const [_, updatedHistory] = finalData;
    const lastMessage = updatedHistory[updatedHistory.length - 1];
    const botResponse = lastMessage ? lastMessage[1] : "No response";
    
    const frontendResponse = {
      message: {
        role: "assistant",
        content: botResponse,
        timestamp: new Date()  // Fixed: Return Date object instead of ISO string
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
