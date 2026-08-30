import { convertFileSrc } from "@tauri-apps/api/core";

const ROUTES = new Set([
  "capture",
  "thumbnail",
  "annotation-source",
  "asset",
  "media",
]);

function checkedSegment(segment) {
  if (
    typeof segment !== "string" ||
    segment.length === 0 ||
    segment === "." ||
    segment === ".." ||
    /[/\\?#]/.test(segment)
  ) {
    throw new TypeError("Invalid Kiri resource URL segment.");
  }
  return segment;
}

/**
 * Builds a platform-correct URL for the private `kiri` protocol.
 * Tauri percent-encodes the complete route and uses an HTTP origin on Windows,
 * so query parameters are appended only after the platform conversion.
 */
export function kiriResourceUrl(route, segments = [], query) {
  if (!ROUTES.has(route)) {
    throw new TypeError("Unknown Kiri resource URL route.");
  }
  const joinedRoute = [route, ...segments.map(checkedSegment)].join("/");
  const encodedQuery = new URLSearchParams();
  for (const [key, value] of Object.entries(query ?? {})) {
    checkedSegment(key);
    encodedQuery.append(key, String(value));
  }
  const converted = convertFileSrc(joinedRoute, "kiri");
  const suffix = encodedQuery.toString();
  return suffix ? `${converted}?${suffix}` : converted;
}
