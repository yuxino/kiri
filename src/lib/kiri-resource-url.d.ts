export type KiriResourceRoute =
  | "capture"
  | "thumbnail"
  | "annotation-source"
  | "asset"
  | "media";

export function kiriResourceUrl(
  route: KiriResourceRoute,
  segments?: readonly string[],
  query?: Readonly<Record<string, string | number>>,
): string;
