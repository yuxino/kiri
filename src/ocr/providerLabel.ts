import { t } from "../i18n";
import type { OcrProviderPreset } from "../lib/ipc";

export function ocrProviderLabel(provider: OcrProviderPreset): string {
  switch (provider) {
    case "aliyunBailian":
      return t("Alibaba Cloud Model Studio");
    case "openAi":
      return "OpenAI";
    case "customOpenAi":
      return t("Custom OpenAI-compatible");
  }
}
