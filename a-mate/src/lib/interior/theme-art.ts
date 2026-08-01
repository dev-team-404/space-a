const files = import.meta.glob<string>(
  '../../assets/interior/theme-backgrounds/*.png',
  { eager: true, query: '?url', import: 'default' },
);

export const THEME_BACKGROUND_BY_ID: Readonly<Record<string, string>> = Object.freeze(
  Object.fromEntries(
    Object.entries(files).map(([path, url]) => [path.split('/').pop()!.replace(/\.png$/, ''), url]),
  ),
);
