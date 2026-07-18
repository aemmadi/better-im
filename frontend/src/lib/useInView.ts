// A sentinel-based infinite-scroll trigger. Attach the returned ref to an empty
// element at the end of a scrollable list; `onInView` fires whenever it scrolls
// into view (with a generous margin so the next page loads before the user hits
// the bottom). Used by the non-virtualized Gallery / Links grids.

import { useEffect, useRef } from "react";

export function useInView<T extends HTMLElement = HTMLDivElement>(
  onInView: () => void,
  enabled: boolean,
) {
  const ref = useRef<T>(null);
  const cb = useRef(onInView);
  cb.current = onInView;

  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) cb.current();
      },
      { rootMargin: "400px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [enabled]);

  return ref;
}
