import { Links, Meta, Outlet, Scripts, ScrollRestoration, isRouteErrorResponse } from "react-router";
import type { Route } from "./+types/root";
import "./app.css";

const FONTS = "https://fonts.googleapis.com/css2?family=Geist:wght@300..600&family=Geist+Mono:wght@400;500&display=swap";

export const links: Route.LinksFunction = () => [
  { rel: "icon", type: "image/svg+xml", href: `${import.meta.env.BASE_URL}favicon.svg` },
  { rel: "icon", type: "image/png", sizes: "32x32", href: `${import.meta.env.BASE_URL}favicon-32.png` },
  { rel: "apple-touch-icon", href: `${import.meta.env.BASE_URL}apple-touch-icon.png` },
  { rel: "preconnect", href: "https://fonts.googleapis.com" },
  { rel: "preconnect", href: "https://fonts.gstatic.com", crossOrigin: "anonymous" },
  { rel: "stylesheet", href: FONTS },
];

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#000000" />
        <Meta />
        <Links />
      </head>
      <body>
        {children}
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function Root() {
  return <Outlet />;
}

export function ErrorBoundary({ error }: Route.ErrorBoundaryProps) {
  const message = isRouteErrorResponse(error) && error.status === 404 ? "404" : "Error";
  return (
    <main className="grid min-h-svh place-items-center font-mono text-sm text-ink-dim">
      {message}
    </main>
  );
}
