import { Links, Meta, Outlet, Scripts, ScrollRestoration, isRouteErrorResponse } from "react-router";
import type { Route } from "./+types/root";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./app.css";

const base = import.meta.env.BASE_URL;

export const links: Route.LinksFunction = () => [
  { rel: "icon", type: "image/svg+xml", href: `${base}favicon.svg` },
  { rel: "icon", type: "image/png", sizes: "96x96", href: `${base}favicon-96.png` },
  { rel: "icon", type: "image/png", sizes: "48x48", href: `${base}favicon-48.png` },
  { rel: "icon", type: "image/png", sizes: "32x32", href: `${base}favicon-32.png` },
  { rel: "apple-touch-icon", href: `${base}apple-touch-icon.png` },
  { rel: "manifest", href: `${base}manifest.webmanifest` },
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
