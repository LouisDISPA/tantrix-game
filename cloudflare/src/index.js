// The Rust module provides both server rendering and the authoritative room.
import SsrEntrypoint, { Room, RoomCodes } from "../build/ssr/index.js";

export { Room, RoomCodes };

const SITE_ORIGIN = "https://tantrix.louis-dispa.fr";
const PUBLIC_PAGES = ["/", "/learn", "/about", "/online"];

function withSecurityHeaders(response, url) {
  const secured = new Response(response.body, response);
  const socketOrigin = `${url.protocol === "https:" ? "wss:" : "ws:"}//${url.host}`;

  // The requested SSR URL determines its own search directives. Dioxus 0.7
  // head links and meta tags ignore prop changes after their first render.
  if (secured.headers.get("Content-Type")?.includes("text/html")) {
    if (PUBLIC_PAGES.includes(url.pathname)) {
      secured.headers.append("Link", `<${SITE_ORIGIN}${url.pathname}>; rel="canonical"`);
    } else {
      secured.headers.set("X-Robots-Tag", "noindex");
    }
  }

  secured.headers.set("X-Content-Type-Options", "nosniff");
  secured.headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  secured.headers.set("X-Frame-Options", "DENY");
  secured.headers.set("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
  // Dioxus SSR emits inline streaming and hydration scripts. Its board also
  // uses inline styles, and the client initializes a WebAssembly module.
  secured.headers.set(
    "Content-Security-Policy",
    [
      "default-src 'self'",
      "base-uri 'self'",
      "object-src 'none'",
      "frame-ancestors 'none'",
      "form-action 'self'",
      "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'",
      "style-src 'self' 'unsafe-inline'",
      "img-src 'self' data:",
      `connect-src 'self' ${socketOrigin} https://api.github.com`,
    ].join("; "),
  );
  if (url.protocol === "https:") {
    secured.headers.set("Strict-Transport-Security", "max-age=31536000");
  }

  return secured;
}

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);

    if (url.pathname === "/health") {
      return withSecurityHeaders(new Response("OK"), url);
    }

    if (url.pathname === "/robots.txt") {
      return withSecurityHeaders(
        new Response(`User-agent: *\nAllow: /\nSitemap: ${SITE_ORIGIN}/sitemap.xml\n`, {
          headers: { "Content-Type": "text/plain; charset=utf-8" },
        }),
        url,
      );
    }

    if (url.pathname === "/sitemap.xml") {
      const pages = PUBLIC_PAGES.map(
        (path) => `  <url><loc>${SITE_ORIGIN}${path}</loc></url>`,
      ).join("\n");
      return withSecurityHeaders(
        new Response(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${pages}\n</urlset>\n`, {
          headers: { "Content-Type": "application/xml; charset=utf-8" },
        }),
        url,
      );
    }

    // Existing static files are served by the assets layer before the Worker.
    if (/\.[^/]*$/.test(url.pathname)) {
      return withSecurityHeaders(await env.ASSETS.fetch(request), url);
    }

    try {
      const response = await new SsrEntrypoint(ctx, env).fetch(request);
      return response.status === 101 ? response : withSecurityHeaders(response, url);
    } catch (error) {
      console.error(JSON.stringify({ message: "SSR failed", path: url.pathname, error: String(error) }));
      return withSecurityHeaders(new Response("Internal Server Error", { status: 500 }), url);
    }
  },
};
