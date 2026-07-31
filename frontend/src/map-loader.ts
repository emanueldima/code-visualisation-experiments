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
