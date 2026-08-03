export function createMapLoader<T>(all: T, loadPublic: () => Promise<T>) {
  let publicMap: Promise<T> | undefined;
  return (publicOnly: boolean): Promise<T> => {
    if (!publicOnly) return Promise.resolve(all);
    publicMap ??= loadPublic().catch((error: unknown) => {
      publicMap = undefined;
      throw error;
    });
    return publicMap;
  };
}

export function createCachedLoader<T>(
  load: (key: string) => Promise<T>,
  initial?: readonly [string, T],
) {
  const cache = new Map<string, Promise<T>>();
  if (initial) cache.set(initial[0], Promise.resolve(initial[1]));
  return (key: string): Promise<T> => {
    let value = cache.get(key);
    if (value) return value;
    value = load(key).catch((error: unknown) => {
      cache.delete(key);
      throw error;
    });
    cache.set(key, value);
    return value;
  };
}
