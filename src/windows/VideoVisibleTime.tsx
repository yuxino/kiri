import {t} from "../i18n";
import {projectTimedRange, videoTimeLabel, type VideoSegment} from "./video-trim.js";

/** The collapsed inspector uses the same clock as the finished-video timeline. */
export function VideoVisibleTime({segments, start, end}: {segments: VideoSegment[]; start: number; end: number}) {
  const ranges: {start: number; end: number}[] = [];
  for (const range of projectTimedRange(segments, start, end)) {
    const previous = ranges[ranges.length - 1];
    if (previous && Math.abs(previous.end - range.start) < .001) previous.end = range.end;
    else ranges.push({start: range.start, end: range.end});
  }
  return <output className="kiri-video-visible-times">{ranges.length
    ? ranges.map((range, index) => <span key={index}>{videoTimeLabel(range.start)} – {videoTimeLabel(range.end)}</span>)
    : t("Outside the edit")}</output>;
}
