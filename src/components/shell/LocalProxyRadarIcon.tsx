import { useEffect, useRef, useState, type CSSProperties } from "react";
import { cn } from "@/lib/utils";
import sprite24Light from "@/assets/routing-globe/routing-globe-sprite-24-light.png";
import sprite24Dark from "@/assets/routing-globe/routing-globe-sprite-24-dark.png";
import sprite32Light from "@/assets/routing-globe/routing-globe-sprite-32-light.png";
import sprite32Dark from "@/assets/routing-globe/routing-globe-sprite-32-dark.png";
import static24Light from "@/assets/routing-globe/routing-globe-static-24-light.png";
import static24Dark from "@/assets/routing-globe/routing-globe-static-24-dark.png";
import static32Light from "@/assets/routing-globe/routing-globe-static-32-light.png";
import static32Dark from "@/assets/routing-globe/routing-globe-static-32-dark.png";

type LocalProxyRadarIconProps = {
  active: boolean;
  size?: 24 | 32;
  className?: string;
};

type GlobeStyle = CSSProperties & {
  "--local-proxy-globe-size": string;
  "--local-proxy-globe-end": string;
  "--local-proxy-globe-sprite-light": string;
  "--local-proxy-globe-sprite-dark": string;
  "--local-proxy-globe-static-light": string;
  "--local-proxy-globe-static-dark": string;
};

const sprites = {
  24: {
    light: sprite24Light,
    dark: sprite24Dark,
    staticLight: static24Light,
    staticDark: static24Dark,
  },
  32: {
    light: sprite32Light,
    dark: sprite32Dark,
    staticLight: static32Light,
    staticDark: static32Dark,
  },
} as const;

export function LocalProxyRadarIcon({
  active,
  size = 24,
  className,
}: LocalProxyRadarIconProps) {
  const iconRef = useRef<HTMLSpanElement>(null);
  const [isVisible, setIsVisible] = useState(true);
  const [documentVisible, setDocumentVisible] = useState(() => document.visibilityState !== "hidden");
  const sprite = sprites[size];

  useEffect(() => {
    const node = iconRef.current;
    if (!node) return;

    const handleDocumentVisibility = () => {
      setDocumentVisible(document.visibilityState !== "hidden");
    };
    document.addEventListener("visibilitychange", handleDocumentVisibility);

    if (typeof IntersectionObserver === "undefined") {
      return () => document.removeEventListener("visibilitychange", handleDocumentVisibility);
    }

    const observer = new IntersectionObserver(([entry]) => {
      setIsVisible(entry?.isIntersecting ?? false);
    });
    observer.observe(node);

    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", handleDocumentVisibility);
    };
  }, []);

  const style: GlobeStyle = {
    "--local-proxy-globe-size": `${size}px`,
    "--local-proxy-globe-end": `${-size * 16}px`,
    width: `${size}px`,
    height: `${size}px`,
    "--local-proxy-globe-sprite-light": `url("${sprite.light}")`,
    "--local-proxy-globe-sprite-dark": `url("${sprite.dark}")`,
    "--local-proxy-globe-static-light": `url("${sprite.staticLight}")`,
    "--local-proxy-globe-static-dark": `url("${sprite.staticDark}")`,
  };

  return (
    <span
      ref={iconRef}
      aria-hidden="true"
      className={cn(
        "local-proxy-globe",
        active && isVisible && documentVisible && "local-proxy-globe--active",
        className,
      )}
      data-state={active ? "active" : "idle"}
      data-visible={isVisible && documentVisible ? "true" : "false"}
      style={style}
    />
  );
}
