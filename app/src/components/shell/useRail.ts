import { useEffect, useState } from "react";

const query = "(max-width: 1000px)";

/** Narrow windows turn the sidebar into an icon rail so the work area keeps its width. */
export function useRail() {
  const [rail, setRail] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const on = () => setRail(media.matches);
    media.addEventListener("change", on);
    return () => media.removeEventListener("change", on);
  }, []);
  return rail;
}
