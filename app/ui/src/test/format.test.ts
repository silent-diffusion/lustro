import { describe, expect, it } from "vitest";
import { fmtAge, fmtBytes, fmtCount, fmtDuration, fmtPct, fmtWhen } from "../lib/format";

describe("fmtBytes", () => {
  it("uses decimal units like the mockups", () => {
    expect(fmtBytes(0)).toBe("0 B");
    expect(fmtBytes(999)).toBe("999 B");
    expect(fmtBytes(1500)).toBe("1.5 KB");
    expect(fmtBytes(340e6)).toBe("340 MB");
    expect(fmtBytes(15.3e9)).toBe("15.3 GB");
    expect(fmtBytes(512e9)).toBe("512 GB");
    expect(fmtBytes(2e12)).toBe("2.00 TB");
  });
  it("signs changes", () => {
    expect(fmtBytes(4.1e9, { sign: true })).toBe("+4.1 GB");
    expect(fmtBytes(-2.1e9, { sign: true })).toBe("−2.1 GB");
    expect(fmtBytes(-2.1e9)).toBe("−2.1 GB");
    expect(fmtBytes(0, { sign: true })).toBe("0 B");
  });
  it("supports binary units", () => {
    expect(fmtBytes(1024 ** 3, { binary: true })).toBe("1.0 GiB");
    expect(fmtBytes(1536, { binary: true })).toBe("1.5 KiB");
  });
});

describe("other formatters", () => {
  it("formats counts, percents and durations", () => {
    expect(fmtCount(1284)).toBe("1,284");
    expect(fmtPct(0.1234)).toBe("12.3%");
    expect(fmtPct(Infinity)).toBe("–");
    expect(fmtDuration(65_000)).toBe("1:05");
  });
  it("formats dates relative to now", () => {
    const now = Date.UTC(2026, 9, 28, 12) / 1000;
    expect(fmtAge(now - 3 * 86400, now)).toBe("3 days ago");
    expect(fmtAge(now - 100 * 86400, now)).toBe("3 months ago");
    expect(fmtAge(null, now)).toBe("—");
    expect(fmtWhen(Date.UTC(2025, 2, 14, 12) / 1000, now)).toBe("Mar 14, 2025");
    expect(fmtWhen(now, now).startsWith("Today")).toBe(true);
  });
});
