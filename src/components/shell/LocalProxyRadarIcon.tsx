import { cn } from "@/lib/utils";

type LocalProxyRadarIconProps = {
  active: boolean;
  className?: string;
};

export function LocalProxyRadarIcon({ active, className }: LocalProxyRadarIconProps) {
  return (
    <svg
      aria-hidden="true"
      className={cn("local-proxy-radar", active && "local-proxy-radar--active", className)}
      data-state={active ? "active" : "idle"}
      fill="none"
      viewBox="0 0 24 24"
    >
      <circle className="local-proxy-radar__pulse" cx="12" cy="12" r="4.25" />
      <path className="local-proxy-radar__wave" d="M8.9 8.8a4.6 4.6 0 0 0 0 6.4" />
      <path className="local-proxy-radar__wave" d="M15.1 8.8a4.6 4.6 0 0 1 0 6.4" />
      <path className="local-proxy-radar__wave" d="M6.1 6.4a8.1 8.1 0 0 0 0 11.2" />
      <path className="local-proxy-radar__wave" d="M17.9 6.4a8.1 8.1 0 0 1 0 11.2" />
      <circle className="local-proxy-radar__core" cx="12" cy="12" r="1.65" />
    </svg>
  );
}
