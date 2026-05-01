import WebSocket from 'ws';

console.log("==================================================");
console.log("📱 Mobile Companion E2E Simulator");
console.log("==================================================");

const SERVER_URL = 'ws://localhost:9001';
const TOKEN = 'test_token_123'; // Matches default mock token or can be anything if not verified

let ws = new WebSocket(SERVER_URL);

ws.on('open', () => {
  console.log(`[Mobile] Connected to ${SERVER_URL}`);
  
  // 1. Send authentication
  const authMsg = {
    type: 'auth',
    token: TOKEN
  };
  console.log('[Mobile] Sending Auth:', authMsg);
  ws.send(JSON.stringify(authMsg));
  console.log('✅ Authentication sent! Waiting for HITL requests...');
  console.log('👉 Hãy lên Desktop App, chạy một workflow yêu cầu HITL.');
});

ws.on('message', (data) => {
  try {
    const msg = JSON.parse(data.toString());
    console.log('\n[Mobile] Received Message:', JSON.stringify(msg, null, 2));

    if (msg.type === 'approval_request') {
      console.log('🚨 Approval Request Received!');
      console.log(`   Action ID: ${msg.action_id}`);
      console.log(`   Risk: ${msg.risk_level}`);
      console.log(`   Description: ${msg.description}`);
      
      // Automatically approve after 2 seconds
      setTimeout(() => {
        const responseMsg = {
          type: 'approval_response',
          action_id: msg.action_id,
          approved: true,
          reason: "Approved via E2E Test Script!",
          responded_by: "e2e_test_script"
        };
        console.log('\n[Mobile] Auto-Approving Request...');
        console.log('[Mobile] Sending:', responseMsg);
        ws.send(JSON.stringify(responseMsg));
      }, 2000);
    }
  } catch (e) {
    console.error('[Mobile] Failed to parse message:', data.toString());
  }
});

ws.on('error', (err) => {
  console.error('[Mobile] WebSocket Error:', err.message);
});

ws.on('close', () => {
  console.log('[Mobile] Connection closed.');
});
