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
    const handleWindowFocus = () => {
      // WebView2 can deliver the first intersection batch while the window is
      // still being shown. A focused window is visible to the user even when
      // that initial visibility signal was stale.
      if (document.visibilityState !== "hidden") {
        setDocumentVisible(true);
      }
    };
    document.addEventListener("visibilitychange", handleDocumentVisibility);
    window.addEventListener("focus", handleWindowFocus);
    window.addEventListener("pageshow", handleWindowFocus);

    if (typeof IntersectionObserver === "undefined") {
      return () => {
        document.removeEventListener("visibilitychange", handleDocumentVisibility);
        window.removeEventListener("focus", handleWindowFocus);
        window.removeEventListener("pageshow", handleWindowFocus);
      };
    }

    const observer = new IntersectionObserver(([entry]) => {
      // Keep the previous visibility value if the browser delivers an empty
      // batch. A malformed/transient observer callback must not turn an
      // active icon into a permanently static one.
      if (entry) {
        // A partial WebView shim may omit geometry. There is no safe way to
        // conclude that the icon is off-screen without a rectangle, so retain
        // the current visible state until a complete entry arrives.
        if (!entry.isIntersecting && !entry.boundingClientRect) {
          return;
        }
        if (!entry.isIntersecting && isTransientlyVisible(entry)) {
          return;
        }
        setIsVisible(entry.isIntersecting);
      }
    });
    observer.observe(node);

    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", handleDocumentVisibility);
      window.removeEventListener("focus", handleWindowFocus);
      window.removeEventListener("pageshow", handleWindowFocus);
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
        active && "local-proxy-globe--active",
        active && (!isVisible || !documentVisible) && "local-proxy-globe--paused",
        className,
      )}
      data-state={active ? "active" : "idle"}
      data-visible={isVisible && documentVisible ? "true" : "false"}
      style={style}
    />
  );
}

function isTransientlyVisible(entry: IntersectionObserverEntry): boolean {
  const { boundingClientRect } = entry;
  // Some WebView/test shims omit the rect even though the entry itself is
  // valid. Treat that as an ordinary non-intersecting result instead of
  // throwing from the observer callback and leaving the component stuck.
  if (!boundingClientRect || boundingClientRect.width <= 0 || boundingClientRect.height <= 0) {
    return false;
  }

  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;
  if (viewportWidth <= 0 || viewportHeight <= 0) {
    return false;
  }

  return (
    boundingClientRect.right > 0 &&
    boundingClientRect.bottom > 0 &&
    boundingClientRect.left < viewportWidth &&
    boundingClientRect.top < viewportHeight
  );
}
