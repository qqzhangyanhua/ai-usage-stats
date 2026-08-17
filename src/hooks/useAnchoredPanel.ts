import { useLayoutEffect, useState, type CSSProperties, type RefObject } from "react";

type Align = "left" | "right";

/** 按触发器位置用 fixed 定位浮层，避免被 overflow:hidden 裁切。 */
export function useAnchoredPanel(
  open: boolean,
  rootRef: RefObject<HTMLElement | null>,
  align: Align = "right",
  estimatedHeight = 240,
): CSSProperties {
  const [style, setStyle] = useState<CSSProperties>({});

  useLayoutEffect(() => {
    if (!open || !rootRef.current) {
      return;
    }
    function place() {
      const root = rootRef.current;
      if (!root) {
        return;
      }
      const rect = root.getBoundingClientRect();
      const openUp =
        window.innerHeight - rect.bottom < estimatedHeight && rect.top > estimatedHeight;
      const next: CSSProperties = {
        position: "fixed",
        zIndex: 40,
        minWidth: Math.max(rect.width, 168),
        maxHeight: Math.min(320, window.innerHeight - 24),
      };
      if (align === "left") {
        next.left = Math.max(8, rect.left);
      } else {
        next.right = Math.max(8, window.innerWidth - rect.right);
      }
      if (openUp) {
        next.bottom = window.innerHeight - rect.top + 6;
      } else {
        next.top = rect.bottom + 6;
      }
      setStyle(next);
    }
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [align, estimatedHeight, open, rootRef]);

  return style;
}
