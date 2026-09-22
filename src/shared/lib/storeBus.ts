// Cross-store refresh signal. Two stores own the proxy list; importing one
// from the other would close a cycle, so they meet here instead.
type Topic = "proxies" | "profiles" | "extensions";

const subs: Record<Topic, Set<() => void>> = {
  proxies: new Set(),
  profiles: new Set(),
  extensions: new Set(),
};

export const storeBus = {
  on(topic: Topic, fn: () => void): () => void {
    subs[topic].add(fn);
    return () => subs[topic].delete(fn);
  },
  emit(topic: Topic): void {
    for (const fn of subs[topic]) fn();
  },
};
