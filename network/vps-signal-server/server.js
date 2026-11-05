const WebSocket = require('ws');
const http = require('http');

const PORT = process.env.PORT || 4003;

// Map of EVM address → WebSocket connection
const sessions = new Map();

// In-memory queue for offline signals. (Key: address, Value: Array of message objects)
// Since this is just WebRTC signaling Offer/Answers, keeping it in memory is perfectly fine 
// and much faster/cheaper than a database. They expire after 5 minutes anyway.
const messageQueue = new Map();

function deliverInstantMessage(recipient, payload) {
    const recipientWs = sessions.get(recipient);
    if (recipientWs && recipientWs.readyState === WebSocket.OPEN) {
        try {
            recipientWs.send(JSON.stringify(payload));
            return true;
        } catch (_) {
            sessions.delete(recipient);
        }
    }
    return false;
}

function queueOfflineMessage(recipient, payload) {
    let queue = messageQueue.get(recipient) || [];
    queue.push(payload);
    const fiveMinAgo = Date.now() - 5 * 60 * 1000;
    queue = queue.filter(m => m.ts > fiveMinAgo).slice(-50);
    messageQueue.set(recipient, queue);
}

const server = http.createServer((req, res) => {
    // Basic HTTP health-check for NGINX/Cloudflare
    if (req.url === '/health' || req.url === '/') {
        res.writeHead(200);
        res.end('Cipher VPS Signal Server Online');
        return;
    }

    // Co-located cipher-relay calls this after POST /store (127.0.0.1 only).
    if (req.method === 'POST' && req.url === '/internal/push') {
        const remote = req.socket.remoteAddress || '';
        const localOnly =
            remote === '127.0.0.1' ||
            remote === '::1' ||
            remote === '::ffff:127.0.0.1';
        if (!localOnly) {
            res.writeHead(403);
            res.end('Forbidden');
            return;
        }
        let body = '';
        req.on('data', chunk => { body += chunk; });
        req.on('end', () => {
            try {
                const data = JSON.parse(body);
                const recipient = (data.to || '').toLowerCase();
                if (!recipient) {
                    res.writeHead(400);
                    res.end('Missing to');
                    return;
                }
                const msgType = data.type || 'message';
                const payload = {
                    type: msgType,
                    message_id: data.message_id,
                    data: data.data,
                    ts: Date.now(),
                };
                if (deliverInstantMessage(recipient, payload)) {
                    res.writeHead(200, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify({ delivered: true }));
                } else {
                    queueOfflineMessage(recipient, payload);
                    res.writeHead(200, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify({ delivered: false, queued: true }));
                }
            } catch (e) {
                res.writeHead(400);
                res.end('Invalid JSON');
            }
        });
        return;
    }

    res.writeHead(404);
    res.end();
});

const wss = new WebSocket.Server({ server, path: '/ws' });

wss.on('connection', (ws, req) => {
    try {
        const url = new URL(req.url, `http://${req.headers.host}`);
        const addr = (url.searchParams.get('addr') || '').toLowerCase();

        if (!addr) {
            ws.close(1008, 'Missing addr parameter');
            return;
        }

        // Close any existing connection for this address (single-device model)
        const existing = sessions.get(addr);
        if (existing) {
            try { existing.close(1000, 'Replaced by new connection'); } catch (_) {}
        }

        // Register the new WebSocket
        sessions.set(addr, ws);
        console.log(`[Signal] ${addr} connected (${sessions.size} total peers)`);

        // Deliver any queued offline messages instantly
        const queued = messageQueue.get(addr);
        if (queued && Array.isArray(queued)) {
            for (const msg of queued) {
                try {
                    ws.send(JSON.stringify(msg));
                } catch (_) {}
            }
            messageQueue.delete(addr);
            console.log(`[Signal] Delivered ${queued.length} queued messages to ${addr}`);
        }

        // Handle incoming messages from this peer
        ws.on('message', (message) => {
            try {
                const data = JSON.parse(message);
                
                // Allow "ping" application keep-alives silently to keep NGINX/CF connections hot
                if (data.type === 'ping') {
                    // We don't need to reply, just acknowledging it keeps the socket alive
                    return;
                }

                const recipient = (data.to || '').toLowerCase();
                const payload = data.payload;

                if (!recipient || !payload) {
                    ws.send(JSON.stringify({ error: 'Missing to/payload fields' }));
                    return;
                }

                // Try instant delivery to recipient
                const signalPayload = {
                    from: addr,
                    payload: payload,
                    ts: Date.now(),
                };
                if (deliverInstantMessage(recipient, signalPayload)) {
                    ws.send(JSON.stringify({ ack: true, delivered: true }));
                    return;
                }

                queueOfflineMessage(recipient, signalPayload);

                ws.send(JSON.stringify({ ack: true, delivered: false, queued: true }));
            } catch (e) {
                ws.send(JSON.stringify({ error: 'Invalid message format' }));
            }
        });

        // Handle disconnect
        ws.on('close', () => {
            if (sessions.get(addr) === ws) {
                sessions.delete(addr);
            }
            console.log(`[Signal] ${addr} disconnected (${sessions.size} total peers)`);
        });

        ws.on('error', () => {
            if (sessions.get(addr) === ws) {
                sessions.delete(addr);
            }
        });

    } catch (e) {
        console.error('[Signal] Connection error:', e);
        ws.close(1011, 'Internal Server Error');
    }
});

// Periodic cleanup of stale queues every 5 minutes
setInterval(() => {
    const fiveMinAgo = Date.now() - 5 * 60 * 1000;
    let culled = 0;
    for (const [addr, queue] of messageQueue.entries()) {
        const filtered = queue.filter(m => m.ts > fiveMinAgo);
        if (filtered.length === 0) {
            messageQueue.delete(addr);
            culled++;
        } else {
            messageQueue.set(addr, filtered);
        }
    }
    if (culled > 0) console.log(`[Signal Cleanup] Removed ${culled} expired queues.`);
}, 5 * 60 * 1000);

server.listen(PORT, () => {
    console.log(`🚀 Cipher VPS Signal Server running on port ${PORT}`);
    console.log(`🌐 Ready to accept WebSocket connections at ws://localhost:${PORT}/ws`);
});
