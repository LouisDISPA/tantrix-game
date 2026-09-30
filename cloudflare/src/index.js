// The Rust module provides both server rendering and the authoritative room.
import SsrEntrypoint, { Room } from "../build/ssr/index.js";

export { Room };

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);

    if (url.pathname === "/health") {
      return new Response("OK");
    }

    if (request.headers.get("Upgrade")?.toLowerCase() === "websocket") {
      const match = url.pathname.match(/^\/room\/([ABCDEFGHJKLMNPRSTUVWXYZ]{6})$/);
      if (!match) {
        return new Response("invalid room code", { status: 400 });
      }
      return env.ROOM.getByName(match[1]).fetch(request);
    }

    // Existing static files are served by the assets layer before the Worker.
    if (/\.[^/]*$/.test(url.pathname)) {
      return env.ASSETS.fetch(request);
    }

    try {
      return await new SsrEntrypoint(ctx, env).fetch(request);
    } catch (error) {
      console.error(JSON.stringify({ message: "SSR failed", path: url.pathname, error: String(error) }));
      return new Response("Internal Server Error", { status: 500 });
    }
  },
};
