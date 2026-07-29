import { beforeEach, describe, expect, it } from "vitest";

import { LANGS, STRINGS, detectLang, lang, setLang, t } from "./i18n";

beforeEach(() => setLang("en"));

describe("detectLang", () => {
  it("prefers what the user chose over what the browser reports", () => {
    expect(detectLang("ja", "pt-BR")).toBe("ja");
  });

  it("falls back to the browser's language when nothing was chosen", () => {
    expect(detectLang(null, "pt-BR")).toBe("pt");
    expect(detectLang(null, "DE-de")).toBe("de");
  });

  it("lands on English for anything unsupported, saved or not", () => {
    expect(detectLang(null, "sv-SE")).toBe("en");
    expect(detectLang("klingon", "sv-SE")).toBe("en");
  });
});

describe("t", () => {
  it("returns the current language, and follows setLang", () => {
    expect(t("working")).toBe("Working");

    setLang("pt");

    expect(lang()).toBe("pt");
    expect(t("working")).toBe("Trabalhando");
  });

  it("substitutes every named parameter", () => {
    expect(t("trayWaiting", { n: 3 })).toBe("3 waiting");
  });

  it("degrades to English, then to the key itself — never to a blank", () => {
    setLang("ar");

    expect(t("no-such-key-anywhere")).toBe("no-such-key-anywhere");
  });
});

describe("the ten locales", () => {
  it("all carry every key English does, so no language shows a raw key", () => {
    const english = Object.keys(STRINGS.en);

    for (const code of LANGS) {
      expect(english.filter((k) => !(k in STRINGS[code])), `${code} missing`).toEqual([]);
    }
  });

  it("has no key present in a translation but absent from English", () => {
    const english = new Set(Object.keys(STRINGS.en));

    for (const code of LANGS) {
      expect(Object.keys(STRINGS[code]).filter((k) => !english.has(k)), code).toEqual([]);
    }
  });
});
