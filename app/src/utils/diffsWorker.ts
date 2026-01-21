export function workerFactory() {
  return new Worker(new URL("@pierre/diffs/worker", import.meta.url), {
    type: "module",
  });
}
