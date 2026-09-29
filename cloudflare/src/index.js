import { DurableObject } from "cloudflare:workers";

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/health") {
      return new Response("OK");
    }

    if (request.headers.get("Upgrade")?.toLowerCase() === "websocket") {
      const match = url.pathname.match(/^\/signaling\/(.+)$/);
      if (!match) {
        return new Response("missing room code", { status: 400 });
      }

      const roomCode = match[1];
      const id = env.ROOM.idFromName(roomCode);
      return env.ROOM.get(id).fetch(request);
    }

    return env.ASSETS.fetch(request);
  },
};

export class Room extends DurableObject {
  async fetch(request) {
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("expected websocket", { status: 426 });
    }

    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    const peerId = crypto.randomUUID();

    server.serializeAttachment({ peerId });
    this.ctx.acceptWebSocket(server);

    server.send(JSON.stringify({ IdAssigned: peerId }));
    for (const socket of this.ctx.getWebSockets()) {
      if (socket !== server) {
        socket.send(JSON.stringify({ NewPeer: peerId }));
      }
    }

    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws, message) {
    if (typeof message !== "string") {
      return;
    }

    let request;
    try {
      request = JSON.parse(message);
    } catch {
      return;
    }

    if (request === "KeepAlive") {
      return;
    }

    if (!request.Signal) {
      return;
    }

    const sender = ws.deserializeAttachment()?.peerId;
    const { receiver, data } = request.Signal;
    if (!sender || !receiver) {
      return;
    }

    for (const socket of this.ctx.getWebSockets()) {
      if (socket.deserializeAttachment()?.peerId === receiver) {
        socket.send(JSON.stringify({ Signal: { sender, data } }));
        return;
      }
    }
  }

  async webSocketClose(ws, code, reason) {
    this.broadcastPeerLeft(ws);
    ws.close(code, reason);
  }

  async webSocketError(ws) {
    this.broadcastPeerLeft(ws);
  }

  broadcastPeerLeft(ws) {
    const peerId = ws.deserializeAttachment()?.peerId;
    if (!peerId) {
      return;
    }

    const event = JSON.stringify({ PeerLeft: peerId });
    for (const socket of this.ctx.getWebSockets()) {
      if (socket !== ws) {
        socket.send(event);
      }
    }
  }
}
