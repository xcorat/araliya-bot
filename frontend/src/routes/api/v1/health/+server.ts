import { json } from '@sveltejs/kit';
import type { RequestEvent } from '@sveltejs/kit';

const HF_URL = import.meta.env.VITE_HF_URL || 'https://xcorat-araliya-bot.hf.space';
const HF_TOKEN = import.meta.env.VITE_HF_TOKEN;

export async function GET(event: RequestEvent) {
  try {
    console.log('Health check - HF_URL:', HF_URL);
    console.log('Health check - HF_TOKEN:', HF_TOKEN ? `${HF_TOKEN.substring(0, 10)}...` : 'undefined');
    console.log('Health check - Full URL:', `${HF_URL}/api/v1/health`);
    
    // Forward the request to the HF Space
    const response = await fetch(`${HF_URL}/api/v1/health`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        ...(HF_TOKEN ? { 'Authorization': `Bearer ${HF_TOKEN}` } : {})
      }
    });

    if (!response.ok) {
      const errorText = await response.text();
      console.error(`Error from HF Space health check: ${response.status} - ${errorText}`);
      return json({
        status: 'unhealthy',
        error: `Error from HF Space: ${response.status}`,
        message: errorText
      }, { status: response.status });
    }

    const data = await response.json();
    return json(data);
  } catch (error) {
    console.error('Health check proxy error:', error);
    return json({
      status: 'unhealthy',
      error: 'Internal Server Error',
      message: error instanceof Error ? error.message : 'Unknown error'
    }, { status: 500 });
  }
}
