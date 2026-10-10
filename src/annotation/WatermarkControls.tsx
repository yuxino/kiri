import {t} from "../i18n";
import type {AppearanceSettings} from "./model";
import {AnnotationColors, AnnotationNumberControl} from "./AnnotationControlFields";

/** Content is edited in the canvas. This row only adjusts its appearance. */
export function WatermarkControls(props: {
  appearance: AppearanceSettings; onChange(patch: Partial<AppearanceSettings>, transient?: boolean): void;
  onFinish(): void; onEdit(): void;
}) {
  const a = props.appearance;
  return <>
    <button type="button" className="kiri-annotation-action" onClick={props.onEdit}
      onPointerDown={event => event.preventDefault()}
      onKeyDown={event => { if (event.key === "Enter" || event.key === " ") event.stopPropagation(); }}>{t("Edit watermark")}</button>
    <AnnotationNumberControl label={t("Font")} value={a.watermarkFontSize} min={12} max={128}
      onChange={(watermarkFontSize, transient) => props.onChange({watermarkFontSize}, transient)} onFinish={props.onFinish}/>
    <AnnotationNumberControl label={t("Opacity")} value={a.watermarkOpacity} min={0} max={100} unit="%"
      onChange={(watermarkOpacity, transient) => props.onChange({watermarkOpacity}, transient)} onFinish={props.onFinish}/>
    <AnnotationNumberControl label={t("Rotation")} value={a.watermarkRotation} min={-180} max={180} unit="°"
      onChange={(watermarkRotation, transient) => props.onChange({watermarkRotation}, transient)} onFinish={props.onFinish}/>
    <AnnotationNumberControl label={t("Spacing")} value={a.watermarkSpacing} min={16} max={512}
      onChange={(watermarkSpacing, transient) => props.onChange({watermarkSpacing}, transient)} onFinish={props.onFinish}/>
    <AnnotationColors value={a.watermarkColor} onChange={watermarkColor => props.onChange({watermarkColor})}/>
  </>;
}
