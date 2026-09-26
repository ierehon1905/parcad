import type { Config } from "@react-router/dev/config";

export default {
  ssr: false,
  prerender: ["/"],
  basename: process.env.PARCAD_SITE_BASE ?? "/",
} satisfies Config;
