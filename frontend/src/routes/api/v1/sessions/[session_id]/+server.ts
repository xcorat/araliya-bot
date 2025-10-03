import { json } from '@sveltejs/kit';
import type { RequestEvent } from '@sveltejs/kit';

const HF_URL = import.meta.env.VITE_HF_URL || 'https://xcorat-araliya-bot.hf.space';
const HF_TOKEN = import.meta.env.VITE_HF_TOKEN;

export async function GET(event: RequestEvent) {
  try {
    const { session_id } = event.params;
    
    // Forward the request to the HF Space
    const response = await fetch(`${HF_URL}/api/v1/sessions/${session_id}`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        ...(HF_TOKEN ? { 'Authorization': `Bearer ${HF_TOKEN}` } : {})
      }
    });

    if (!response.ok) {
      const errorText = await response.text();
      console.error(`Error from HF Space: ${response.status} - ${errorText}`);
      return json({
        error: `Error from HF Space: ${response.status}`,
        message: errorText
      }, { status: response.status });
    }

    const data = await response.json();
    return json(data);
  } catch (error) {
    console.error('Proxy error:', error);
    return json({
      error: 'Internal Server Error',
      message: error instanceof Error ? error.message : 'Unknown error'
    }, { status: 500 });
  }
}

export async function DELETE(event: RequestEvent) {
  try {
    const { session_id } = event.params;
    
    // Forward the request to the HF Space
    const response = await fetch(`${HF_URL}/api/v1/sessions/${session_id}`, {
      method: 'DELETE',
      headers: {
        'Content-Type': 'application/json',
        ...(HF_TOKEN ? { 'Authorization': `Bearer ${HF_TOKEN}` } : {})
      }
    });

    if (!response.ok) {
      const errorText = await response.text();
      console.error(`Error from HF Space: ${response.status} - ${errorText}`);
      return json({
        error: `Error from HF Space: ${response.status}`,
        message: errorText
      }, { status: response.status });
    }

    const data = await response.json();
    return json(data);
  } catch (error) {
    console.error('Proxy error:', error);
    return json({
      error: 'Internal Server Error',
      message: error instanceof Error ? error.message : 'Unknown error'
    }, { status: 500 });
  }
}
