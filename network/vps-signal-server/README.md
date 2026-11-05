# Cipher VPS Signal Server

This folder contains a fully functional Node.js WebRTC Signaling server designed to replace the expensive Cloudflare Durable Object Workers.

By hosting this alongside your existing Rust `cipher-relay`, you can move all of your high-volume WebRTC signaling and text message polling behind an unlimited and fully free Cloudflare DNS proxy (Orange Cloud).

## Deployment Instructions

### 1. Push Context & Run with Docker
Upload this folder to your VPS (e.g. `/root/worker-cipher/vps-signal-server`), then start the container:
```bash
cd vps-signal-server
./deploy.sh
```
This script will use `docker-compose up -d --build` to automatically package the `server.js` backend and run it silently in the background.

The signaling server will now be listening internally on `127.0.0.1:4003`. 
(Your Rust relay should already be running locally on port `4002`).

### 2. Configure aaPanel Reverse Proxy
Since you are using aaPanel, you should configure NGINX directly via the aaPanel Web Dashboard:

1. **Add Site:** First, add a new website domain for `signal.tokenkickstarter.com` through the "Website -> Add Site" dialog. Keep PHP processing disabled and ensure an SSL certificate is generated or applied.

2. **Reverse Proxy Rules:** Open the site settings for `signal.tokenkickstarter.com` and go to the **Configuration File** editor. Ensure it contains the following two Location Proxy rules instead of the defaults:

```nginx
    # 1. Route WebRTC Signaling WebSockets to Node.js Docker container
    location /ws {
        proxy_pass http://127.0.0.1:4003;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "Upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_read_timeout 86400;
        proxy_send_timeout 86400;
    }

    # 2. Route Text Message Relay REST API to Rust
    location /relay {
        proxy_pass http://127.0.0.1:4002/relay;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        
        # Disable caching for real-time polling
        add_header Cache-Control "no-store, no-cache, must-revalidate, proxy-revalidate, max-age=0";
    }
```

Save the file and let aaPanel reload NGINX.

### 3. Cloudflare Configuration
1. Log into your Cloudflare Dashboard.
2. Go to DNS -> Records.
3. Add a new **A Record**:
   - Name: `signal`
   - IPv4 Address: `<YOUR_VPS_IP_ADDRESS>`
   - Proxy status: **Proxied (Orange Cloud ON)**

You're done! Your mobile app is now configured to hit `wss://signal.../ws` for signaling and `https://signal.../relay` for messages. Every request will go through Cloudflare's CDN proxy—giving you absolute DOS protection, IP masking, domain fronting, and 0 request costs.
