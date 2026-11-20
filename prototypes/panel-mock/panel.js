/* ==========================================================================
   Nord Stage 4 HA88 — 1:1 Hardware Panel Prototype Builder & Interactivity
   ========================================================================== */

(function () {
  "use strict";

  // ------------------------------------------------------------------------
  // Reusable Hardware Widget Builders (1:1 visual parity with reference photos)
  // ------------------------------------------------------------------------

  function makeKnob({
    label = "",
    badge = "",
    scale = "0-10", // "0-10" | "bipolar10" | "bipolar15" | "freq" | "range" | "encoder" | "master"
    value = 0.5,    // 0..1
    size = "md",    // "md" | "sm" | "enc"
    darkInlay = false,
    morphLed = false,
    extraBottom = ""
  }) {
    const isEncoder = scale === "encoder";
    const isMaster = scale === "master";
    const angle = isEncoder ? 0 : -135 + value * 270;
    const dialClass = size === "sm" ? "knob-dial-area sm" : isEncoder ? "knob-dial-area enc" : "knob-dial-area";

    let svgMarkup = "";
    if (!isEncoder) {
      svgMarkup = buildKnobScaleSvg(scale, size === "sm" ? 44 : 54, size === "sm" ? 42 : 50);
    }

    const bodyClasses = ["knob-body"];
    if (isEncoder) bodyClasses.push("encoder");
    if (isMaster) bodyClasses.push("master-ring");

    const labelMargin = isEncoder ? "4px" : "1px";
    const inner = `
      <div class="${dialClass}">
        ${svgMarkup}
        <div class="${bodyClasses.join(" ")}" data-knob="1" data-value="${value}" data-encoder="${isEncoder ? "1" : "0"}" style="transform: rotate(${angle}deg);">
          ${isEncoder ? "" : `<div class="knob-pointer"></div>`}
        </div>
      </div>
      ${label || badge ? `<div class="row" style="gap:3px; margin-top:${labelMargin};">
        ${morphLed ? `<span class="led sm"></span>` : ""}
        ${label ? `<span class="lbl">${label}</span>` : ""}
        ${badge}
      </div>` : ""}
      ${extraBottom}
    `;

    return `<div class="knob-wrap ${darkInlay ? "knob-dark-inlay" : ""}">${inner}</div>`;
  }

  function buildKnobScaleSvg(scale, w, h) {
    const cx = w / 2;
    const cy = h / 2 + 1;
    const rTick = w === 44 ? 17 : 20;
    const rNum = w === 44 ? 21.5 : 24.5;
    let items = "";

    const polar = (deg, r) => {
      const rad = ((deg - 90) * Math.PI) / 180;
      return [cx + r * Math.cos(rad), cy + r * Math.sin(rad)];
    };

    if (scale === "0-10") {
      for (let i = 0; i <= 10; i++) {
        const deg = -135 + (i / 10) * 270;
        const [x1, y1] = polar(deg, rTick - 1.5);
        const [x2, y2] = polar(deg, rTick + 0.5);
        items += `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="currentColor" stroke-width="0.9" opacity="0.75"/>`;
        if (i % 1 === 0) {
          const [tx, ty] = polar(deg, rNum);
          items += `<text x="${tx.toFixed(1)}" y="${(ty + 2.2).toFixed(1)}" text-anchor="middle" font-size="6" font-weight="700" fill="currentColor" opacity="0.9">${i}</text>`;
        }
      }
    } else if (scale === "bipolar10" || scale === "bipolar15") {
      const labels = scale === "bipolar10"
        ? [{ v: -135, t: "-10" }, { v: -67.5, t: "-5" }, { v: 0, t: "0" }, { v: 67.5, t: "5" }, { v: 135, t: "10" }]
        : [{ v: -135, t: "-15" }, { v: -90, t: "-10" }, { v: -45, t: "-5" }, { v: 0, t: "0" }, { v: 45, t: "5" }, { v: 90, t: "10" }, { v: 135, t: "15" }];
      for (let i = 0; i < labels.length; i++) {
        const { v, t } = labels[i];
        const [x1, y1] = polar(v, rTick - 1.5);
        const [x2, y2] = polar(v, rTick + 0.8);
        const [tx, ty] = polar(v, rNum + 0.5);
        items += `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="currentColor" stroke-width="0.9" opacity="0.75"/>`;
        items += `<text x="${tx.toFixed(1)}" y="${(ty + 2.2).toFixed(1)}" text-anchor="middle" font-size="5.6" font-weight="700" fill="currentColor" opacity="0.9">${t}</text>`;
      }
    } else if (scale === "freq") {
      const stops = [
        { v: -135, t: "200" }, { v: -96, t: "250" }, { v: -58, t: "400" },
        { v: -22, t: "600" }, { v: 12, t: "1K" }, { v: 52, t: "2K" },
        { v: 92, t: "4K" }, { v: 135, t: "8K" }
      ];
      for (const { v, t } of stops) {
        const [tx, ty] = polar(v, rNum + 1);
        items += `<text x="${tx.toFixed(1)}" y="${(ty + 2).toFixed(1)}" text-anchor="middle" font-size="5.5" font-weight="700" fill="currentColor" opacity="0.92">${t}</text>`;
      }
    } else if (scale === "master") {
      // Tapered white volume crescent: razor-thin at -135deg, growing smoothly clockwise to thick wedge at +135deg
      const rIn = 14.5;
      const steps = 36;
      const outerPts = [];
      const innerPts = [];
      for (let i = 0; i <= steps; i++) {
        const t = i / steps;
        const deg = -135 + t * 270;
        const rOut = rIn + 0.3 + 5.8 * Math.pow(t, 1.15);
        const [ox, oy] = polar(deg, rOut);
        const [ix, iy] = polar(deg, rIn);
        outerPts.push(`${ox.toFixed(2)},${oy.toFixed(2)}`);
        innerPts.unshift(`${ix.toFixed(2)},${iy.toFixed(2)}`);
      }
      items += `<polygon points="${outerPts.join(" ")} ${innerPts.join(" ")}" fill="#eef2f6" opacity="0.94"/>`;
    } else if (scale === "range") {
      // Red arc from -100 to +100 with 1, 2, 3, 4
      const [ax1, ay1] = polar(-105, rTick + 1);
      const [ax2, ay2] = polar(105, rTick + 1);
      items += `<path d="M ${ax1.toFixed(1)} ${ay1.toFixed(1)} A ${rTick + 1} ${rTick + 1} 0 1 1 ${ax2.toFixed(1)} ${ay2.toFixed(1)}" fill="none" stroke="#b9262d" stroke-width="6.5" stroke-linecap="round"/>`;
      const rNums = [{ v: -95, t: "1" }, { v: -32, t: "2" }, { v: 32, t: "3" }, { v: 95, t: "4" }];
      for (const { v, t } of rNums) {
        const [tx, ty] = polar(v, rTick + 1);
        items += `<text x="${tx.toFixed(1)}" y="${(ty + 2).toFixed(1)}" text-anchor="middle" font-size="5.8" font-weight="800" fill="#ffffff">${t}</text>`;
      }
      const [lx, ly] = polar(-135, rNum);
      const [rx, ry] = polar(135, rNum);
      items += `<text x="${lx.toFixed(1)}" y="${(ly + 2).toFixed(1)}" text-anchor="middle" font-size="5.8" font-weight="700" fill="currentColor">0</text>`;
      items += `<text x="${rx.toFixed(1)}" y="${(ry + 2).toFixed(1)}" text-anchor="middle" font-size="5.8" font-weight="700" fill="currentColor">10</text>`;
    }

    return `<svg class="knob-scale-svg" viewBox="0 0 ${w} ${h}" style="color:inherit;">${items}</svg>`;
  }

  function makeFaderStrip(value = 0.72) {
    const litCount = Math.round(value * 11);
    let segs = "";
    for (let i = 0; i < 11; i++) {
      segs += `<div class="led-seg ${i < litCount ? "on green" : ""}"></div>`;
    }
    const topPct = 10 + (1 - value) * 80;
    return `
      <div class="fader-strip" data-fader-strip="1" data-value="${value}">
        <div class="fader-track" data-fader-track="1">
          <div class="fader-cap" style="top: ${topPct}%;"></div>
        </div>
        <div class="led-ladder">${segs}</div>
      </div>
    `;
  }

  function makeTriMatrix(leftLabels, rightLabels, activeIndex = 2, groupKey = "") {
    // 3 rows x 2 cols: indices 0..2 left (top..bottom), 3..5 right (top..bottom)
    let leftHtml = "";
    let rightHtml = "";
    let tris = "";
    for (let r = 0; r < 3; r++) {
      leftHtml += `<div class="lbl text-right" style="line-height:1.0;">${leftLabels[r]}</div>`;
      rightHtml += `<div class="lbl text-left" style="line-height:1.0;">${rightLabels[r]}</div>`;
      tris += `<span class="tri left ${activeIndex === r ? "on" : ""}" data-tri-idx="${r}"></span>`;
      tris += `<span class="tri right ${activeIndex === r + 3 ? "on" : ""}" data-tri-idx="${r + 3}"></span>`;
    }
    return `
      <div class="tri-matrix" data-cycle-group="${groupKey}" data-cycle-count="6" data-cycle-index="${activeIndex}">
        <div class="tri-col-left">${leftHtml}</div>
        <div class="tri-window cols-2">${tris}</div>
        <div class="tri-col-right">${rightHtml}</div>
      </div>
    `;
  }

  function makeDrawbar(spec, idx) {
    const { top1, top2, foot, color, val, maxNum = 8 } = spec;
    let nums = "";
    let leds = "";
    for (let n = 1; n <= 8; n++) {
      nums += `<span>${n <= maxNum ? n : ""}</span>`;
      leds += `<div class="led-seg ${n <= val ? "on red" : ""}"></div>`;
    }
    // Cap top position: val 0 -> 6px, val 8 -> 84px
    const capTop = 6 + (val / 8) * 78;
    return `
      <div class="drawbar-unit" data-drawbar="${idx}" data-value="${val}">
        <div class="drawbar-top-lbl">
          <span>${top1}</span>
          <span>${top2}</span>
        </div>
        <div class="drawbar-body">
          <div class="drawbar-slot" data-drawbar-slot="1">
            <div class="drawbar-cap ${color}" style="top:${capTop}px;"></div>
          </div>
          <div class="drawbar-scale-plate">
            <div class="drawbar-nums">${nums}</div>
            <div class="drawbar-leds">${leds}</div>
          </div>
        </div>
        <div class="drawbar-foot-lbl">${foot}</div>
      </div>
    `;
  }

  // ------------------------------------------------------------------------
  // Smooth Rounded S-Curve Stepped Header SVG (real red chassis shows through)
  // ------------------------------------------------------------------------
  function makeHeaderStepSvg(totalW, notchW, id) {
    const h = 30;
    const stepY = 15;
    const xStep = totalW - notchW;
    const r = 6;
    // Uniform radius (r=6) across all corners so left/step/right radii match identically
    const d = [
      `M 0 ${r}`,
      `A ${r} ${r} 0 0 1 ${r} 0`,
      `L ${xStep - r} 0`,
      `A ${r} ${r} 0 0 1 ${xStep} ${r}`,
      `L ${xStep} ${stepY - r}`,
      `A ${r} ${r} 0 0 0 ${xStep + r} ${stepY}`,
      `L ${totalW - r} ${stepY}`,
      `A ${r} ${r} 0 0 1 ${totalW} ${stepY + r}`,
      `L ${totalW} ${h}`,
      `L 0 ${h}`,
      `Z`
    ].join(" ");
    return `
      <svg class="bay-header-svg" viewBox="0 0 ${totalW} ${h}" preserveAspectRatio="none" aria-hidden="true">
        <defs>
          <linearGradient id="hdr-grad-${id}" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stop-color="#bec8d3"/>
            <stop offset="55%" stop-color="#aab6c2"/>
            <stop offset="100%" stop-color="#9ca8b4"/>
          </linearGradient>
        </defs>
        <path d="${d}" fill="url(#hdr-grad-${id})"/>
      </svg>
    `;
  }

  // ------------------------------------------------------------------------
  // Build All 6 Hardware Bays
  // ------------------------------------------------------------------------

  function buildMasterBay() {
    return `
      <section class="bay bay-red bay-master" id="bay-master">
        <div class="col" style="gap:2px;">
          <span class="lbl">MASTER LEVEL</span>
          ${makeKnob({ scale: "master", value: 0.72, size: "md" })}
        </div>

        <div class="rotary-box">
          <div class="rotary-hdr">ROTARY<br/>SPEAKER</div>
          <div class="col w-full" style="gap:0;">
            <div class="row between w-full" style="padding:2px 5px 0;">
              <span></span>
              <div class="row" style="gap:3px;">
                <span class="lbl lbl-xs">ON</span>
                <span class="led on red" data-led="rotary-on"></span>
              </div>
            </div>
            ${makeKnob({ label: "DRIVE", scale: "0-10", value: 0.25, size: "sm" })}
          </div>

          <div class="col" style="gap:3px;">
            <div class="row"><span class="led" data-led="rotary-organ"></span><span class="lbl lbl-xs">ORGAN</span></div>
            <button class="pill-btn grey sm" data-toggle-led="rotary-organ"></button>
            <div class="row"><span class="led" data-led="rotary-mic"></span><span class="lbl lbl-xs">CLOSE MIC ▽</span></div>
          </div>

          <div class="col" style="gap:3px; border-left:1px solid rgba(255,255,255,0.45); padding-left:4px;">
            <div class="row"><span class="led" data-led="rotary-stop"></span><span class="lbl lbl-xs">STOP MODE</span></div>
            <button class="pill-btn sm" data-toggle-led="rotary-stop"></button>
            <span class="lbl lbl-xs">ANGLE</span>
          </div>

          <div class="col" style="gap:3px;">
            <div class="row" style="gap:3px;">
              <span class="led on green" data-led="rotary-slow"></span><span class="lbl lbl-xs">SLOW</span>
              <span class="led" data-led="rotary-fast"></span><span class="lbl lbl-xs">FAST</span>
            </div>
            <button class="pill-btn sm" data-toggle-pair="rotary-slow,rotary-fast"></button>
            <div class="row"><span class="led" data-led="rotary-morph"></span><span class="lbl lbl-xs">MORPH</span></div>
          </div>
        </div>
      </section>
    `;
  }

  function buildOrganBay() {
    const drawbars = [
      { top1: "BASS16", top2: "16'", foot: "16'", color: "black", val: 7, maxNum: 7 },
      { top1: "STR16", top2: "8'", foot: "5⅓'", color: "black", val: 4, maxNum: 7 },
      { top1: "FLUTE8", top2: "4'", foot: "8'", color: "white", val: 8, maxNum: 8 },
      { top1: "OBOE8", top2: "2'", foot: "4'", color: "white", val: 5, maxNum: 7 },
      { top1: "TRMP8", top2: "II", foot: "2⅔'", color: "black", val: 4, maxNum: 7 },
      { top1: "STR8", top2: "III", foot: "2'", color: "white", val: 5, maxNum: 7 },
      { top1: "FLUTE4", top2: "IV", foot: "1⅗'", color: "black", val: 7, maxNum: 7 },
      { top1: "STR4", top2: "", foot: "1⅓'", color: "black", val: 5, maxNum: 5 },
      { top1: "2 2/3", top2: "~ - ~", foot: "1'", color: "white", val: 5, maxNum: 5 }
    ];

    return `
      <section class="bay bay-slate bay-organ" id="bay-organ">
        <div class="bay-header has-notch">
          ${makeHeaderStepSvg(492, 248, "org")}
          <div class="bay-header-title">
            <span class="h-main">ORGAN</span>
            <span class="h-sub">SECTION</span>
          </div>
          <div class="header-fx-focus">
            <span class="lbl lbl-dark lbl-xs">FX FOCUS</span>
            <span class="led" data-led="org-fx-focus"></span>
          </div>
          <div class="header-on-solo">
            <div class="header-on-col">
              <span class="lbl lbl-dark lbl-xs">ON</span>
              <span class="led on red" data-led="org-on"></span>
            </div>
            <button class="pill-btn" data-toggle-led="org-on"></button>
            <span class="lbl lbl-dark lbl-xs">SOLO ▾</span>
          </div>
          <div class="header-red-notch" style="width: 248px;"></div>
        </div>

        <div class="organ-body">
          <!-- Left Faders & Layer Buttons -->
          <div class="section-left-col">
            <div class="faders-row">
              ${makeFaderStrip(0.84)}
              ${makeFaderStrip(0.62)}
            </div>

            <div class="layer-btns-row">
              <div class="layer-btn-col">
                <div class="row" style="gap:2px;"><span class="lbl lbl-sm">A</span><span class="lbl lbl-xs">AUX KB</span><span class="led sm"></span></div>
                <span class="lbl-pill-white"><span class="led sm on green" data-led="org-a"></span>ON/OFF▾</span>
                <button class="pill-btn grey" data-toggle-led="org-a"></button>
                <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">SUSTPED</span></div>
              </div>
              <div class="layer-btn-col">
                <div class="row" style="gap:2px;"><span class="lbl lbl-sm">B</span><span class="lbl lbl-xs">AUX KB</span><span class="led sm"></span></div>
                <span class="lbl-pill-white"><span class="led sm on green" data-led="org-b"></span>ON/OFF▾</span>
                <button class="pill-btn grey" data-toggle-led="org-b"></button>
                <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">PSTICK</span></div>
              </div>
            </div>

            <div class="col" style="gap:2px;">
              <div class="row"><span class="led sm" data-led="org-preset"></span><span class="lbl lbl-xs">PRESET</span></div>
              <button class="pill-btn sm" data-toggle-led="org-preset"></button>
              <span class="lbl lbl-xs">SYNC ▽</span>
            </div>

            <div class="octave-zone-block">
              <span class="lbl lbl-xs">◀ OCTAVE SHIFT ▶</span>
              <div class="row" style="gap:3px;">
                <button class="pill-btn sm" data-zone-shift="org,-1"></button>
                <button class="pill-btn sm" data-zone-shift="org,1"></button>
              </div>
              <span class="lbl lbl-xs">◀ KB ZONE ▶</span>
              <div class="row" style="gap:4px;" data-zone-leds="org">
                <span class="led sm"></span>
                <span class="led sm"></span>
                <span class="led sm on green"></span>
                <span class="led sm on green"></span>
              </div>
            </div>
          </div>

          <!-- Right: 3 Top Groups + 9 Physical Drawbars -->
          <div class="organ-right-col">
            <div class="organ-top-groups">
              <!-- ORGAN MODEL -->
              <div class="group-box col" style="justify-content:center; gap:8px; padding-top:14px;">
                <span class="group-box-title two-line">ORGAN<br/>MODEL</span>
                ${makeTriMatrix(["FARF", "VOX", "B3"], ["PIPE1", "PIPE2", "B3<br/>BASS"], 2, "org-model")}
                <button class="pill-btn" data-cycle-btn="org-model"></button>
              </div>

              <!-- VIB / CHORUS -->
              <div class="group-box col" style="justify-content:center; gap:10px; padding-top:12px;">
                <span class="group-box-title">VIB / CHORUS</span>
                <div class="row around w-full">
                  <button class="pill-btn" data-cycle-btn="org-vib"></button>
                  <div class="col" style="gap:2px;" data-cycle-group="org-vib" data-cycle-count="6" data-cycle-index="2">
                    <div class="row" style="gap:6px;"><span class="lbl lbl-xs">C2</span><span class="lbl lbl-xs">V3</span><span class="lbl lbl-xs">C3</span></div>
                    <div class="tri-window cols-3">
                      <span class="tri up" data-tri-idx="0"></span>
                      <span class="tri up" data-tri-idx="1"></span>
                      <span class="tri up on" data-tri-idx="2"></span>
                      <span class="tri down" data-tri-idx="3"></span>
                      <span class="tri down" data-tri-idx="4"></span>
                      <span class="tri down" data-tri-idx="5"></span>
                    </div>
                    <div class="row" style="gap:6px;"><span class="lbl lbl-xs">V2</span><span class="lbl lbl-xs">C1</span><span class="lbl lbl-xs">V1</span></div>
                  </div>
                </div>
                <div class="row" style="gap:4px;">
                  <div class="col" style="gap:1px;"><span class="lbl lbl-xs">ON</span><span class="led on red" data-led="org-vib-on"></span></div>
                  <button class="pill-btn grey" data-toggle-led="org-vib-on"></button>
                </div>
              </div>

              <!-- B3 PERCUSSION -->
              <div class="group-box col" style="justify-content:center; gap:10px; padding-top:12px;">
                <span class="group-box-title">B3 PERCUSSION</span>
                <div class="row around w-full">
                  <div class="col" style="gap:2px;">
                    <span class="lbl lbl-xs">VOLUME</span>
                    <div class="row"><span class="led sm" data-led="perc-soft"></span><span class="lbl lbl-xs">SOFT</span></div>
                    <button class="pill-btn" data-toggle-led="perc-soft"></button>
                  </div>
                  <div class="col" style="gap:2px;">
                    <span class="lbl lbl-xs">DECAY</span>
                    <div class="row"><span class="led sm on red" data-led="perc-fast"></span><span class="lbl lbl-xs">FAST</span></div>
                    <button class="pill-btn" data-toggle-led="perc-fast"></button>
                  </div>
                  <div class="col" style="gap:2px;">
                    <span class="lbl lbl-xs">HARMONIC</span>
                    <div class="row"><span class="led sm on red" data-led="perc-third"></span><span class="lbl lbl-xs">THIRD</span></div>
                    <button class="pill-btn" data-toggle-led="perc-third"></button>
                  </div>
                </div>
                <div class="row between w-full" style="padding:0 4px;">
                  <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">POLY ▽</span></div>
                  <div class="row" style="gap:4px;">
                    <div class="col" style="gap:1px;"><span class="lbl lbl-xs">ON</span><span class="led" data-led="perc-on"></span></div>
                    <button class="pill-btn grey" data-toggle-led="perc-on"></button>
                  </div>
                </div>
              </div>
            </div>

            <!-- 9 Drawbars -->
            <div class="drawbars-row">
              ${drawbars.map((d, i) => makeDrawbar(d, i)).join("")}
            </div>
          </div>
        </div>
      </section>
    `;
  }

  function buildPianoBay() {
    return `
      <section class="bay bay-slate bay-piano" id="bay-piano">
        <div class="bay-header">
          <div class="bay-header-title">
            <span class="h-main">PIANO</span>
            <span class="h-sub">SECTION</span>
          </div>
          <div class="header-fx-focus" style="margin-left:auto;">
            <span class="lbl lbl-dark lbl-xs">FX FOCUS</span>
            <span class="led" data-led="pno-fx-focus"></span>
          </div>
          <div class="header-on-solo">
            <div class="header-on-col">
              <span class="lbl lbl-dark lbl-xs">ON</span>
              <span class="led on red" data-led="pno-on"></span>
            </div>
            <button class="pill-btn sm" data-toggle-led="pno-on"></button>
            <span class="lbl lbl-dark lbl-xs">SOLO ▾</span>
          </div>
        </div>

        <div class="piano-body">
          <!-- Left Faders + Timbre + Octave -->
          <div class="section-left-col" style="width:104px;">
            <div class="faders-row" style="gap:10px;">
              ${makeFaderStrip(0.86)}
              ${makeFaderStrip(0.60)}
            </div>

            <div class="layer-btns-row" style="gap:5px;">
              <div class="layer-btn-col">
                <div class="row" style="gap:2px;"><span class="lbl lbl-sm">A</span><span class="lbl lbl-xs">AUX KB</span><span class="led sm"></span></div>
                <span class="lbl-pill-white"><span class="led sm on green" data-led="pno-a"></span>ON/OFF▾</span>
                <button class="pill-btn grey sm" data-toggle-led="pno-a"></button>
                <div class="row"><span class="led sm on red"></span><span class="lbl lbl-xs">SUSTPED</span></div>
              </div>
              <div class="layer-btn-col">
                <div class="row" style="gap:2px;"><span class="lbl lbl-sm">B</span><span class="lbl lbl-xs">AUX KB</span><span class="led sm"></span></div>
                <span class="lbl-pill-white"><span class="led sm on green" data-led="pno-b"></span>ON/OFF▾</span>
                <button class="pill-btn grey sm" data-toggle-led="pno-b"></button>
                <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">PSTICK</span></div>
              </div>
            </div>

            <!-- TIMBRE vertical pill -->
            <div class="row" style="gap:5px;">
              <button class="pill-btn vert" data-cycle-leds="pno-t-brt,pno-t-mid,pno-t-sft" data-allow-off="1"></button>
              <div class="col" style="align-items:flex-start; gap:2px;">
                <span class="lbl lbl-xs" style="align-self:center;">TIMBRE</span>
                <div class="row" style="gap:4px;">
                  <div class="col" style="align-items:flex-end; gap:2px;">
                    <div class="row"><span class="lbl lbl-xs">BRIGHT</span><span class="led sm" data-led="pno-t-brt"></span></div>
                    <div class="row"><span class="lbl lbl-xs">MID</span><span class="led sm" data-led="pno-t-mid"></span></div>
                    <div class="row"><span class="lbl lbl-xs">SOFT</span><span class="led sm" data-led="pno-t-sft"></span></div>
                  </div>
                  <div class="col" style="align-items:flex-start; gap:4px;">
                    <span class="lbl lbl-xs">DYNO1</span>
                    <span class="lbl lbl-xs">DYNO2</span>
                  </div>
                </div>
              </div>
            </div>

            <div class="octave-zone-block">
              <span class="lbl lbl-xs">◀ OCTAVE SHIFT ▶</span>
              <div class="row" style="gap:3px;">
                <button class="pill-btn sm" data-zone-shift="pno,-1"></button>
                <button class="pill-btn sm" data-zone-shift="pno,1"></button>
              </div>
              <span class="lbl lbl-xs">◀ KB ZONE ▶</span>
              <div class="row" style="gap:4px;" data-zone-leds="pno">
                <span class="led sm on green"></span>
                <span class="led sm on green"></span>
                <span class="led sm on green"></span>
                <span class="led sm on green"></span>
              </div>
            </div>
          </div>

          <!-- Right: 2x2 Controls + PIANO SELECT -->
          <div class="piano-right-col">
            <div class="piano-grid-2x2">
              <div class="piano-ctrl-cell">
                <span class="lbl lbl-xs">ACOUSTICS</span>
                <div class="col" style="align-items:flex-start; gap:2px;">
                  <div class="row"><span class="led sm" data-led="pno-softrel"></span><span class="lbl lbl-xs">SOFT REL</span></div>
                  <div class="row"><span class="led sm on red" data-led="pno-strres"></span><span class="lbl lbl-xs">STRING RES</span></div>
                </div>
                <button class="pill-btn sm" data-cycle-leds="pno-softrel,pno-strres,pno-pednoise" data-allow-off="1"></button>
                <div class="row"><span class="led sm on red" data-led="pno-pednoise"></span><span class="lbl lbl-xs">PED NOISE ▽</span></div>
              </div>

              <div class="piano-ctrl-cell">
                <span class="lbl lbl-xs">UNISON</span>
                <div class="row" style="gap:3px;">
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="lbl lbl-xs">2</span><span class="led sm" data-led="pno-uni-2"></span></div>
                    <div class="row"><span class="lbl lbl-xs">1</span><span class="led sm" data-led="pno-uni-1"></span></div>
                  </div>
                  <span class="lbl lbl-xs">3</span>
                </div>
                <button class="pill-btn sm" data-cycle-3="pno-uni-1,pno-uni-2"></button>
              </div>

              <div class="piano-ctrl-cell">
                <span class="lbl lbl-xs">KB TOUCH</span>
                <div class="row" style="gap:3px;">
                  <div class="col" style="align-items:flex-end; gap:2px;">
                    <div class="row"><span class="lbl lbl-xs">MED</span><span class="led sm on red" data-led="pno-touch-m"></span></div>
                    <div class="row"><span class="lbl lbl-xs">HEAVY</span><span class="led sm" data-led="pno-touch-h"></span></div>
                  </div>
                  <span class="lbl lbl-xs">LIGHT</span>
                </div>
                <button class="pill-btn sm" data-cycle-3="pno-touch-m,pno-touch-h"></button>
              </div>

              <div class="piano-ctrl-cell">
                <span class="lbl lbl-xs">DYN COMP</span>
                <div class="row" style="gap:3px;">
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="lbl lbl-xs">2</span><span class="led sm" data-led="pno-comp-2"></span></div>
                    <div class="row"><span class="lbl lbl-xs">1</span><span class="led sm" data-led="pno-comp-1"></span></div>
                  </div>
                  <span class="lbl lbl-xs">3</span>
                </div>
                <button class="pill-btn sm" data-cycle-3="pno-comp-1,pno-comp-2"></button>
              </div>
            </div>

            <div class="group-box piano-select-box" style="justify-content:center; gap:9px; padding:14px 4px 8px;">
              <span class="group-box-title">PIANO SELECT</span>
              ${makeTriMatrix(["ELECTRIC", "UPRIGHT", "GRAND"], ["CLAV", "DIGITAL", "MISC"], 2, "pno-cat")}
              <div class="col" style="gap:3px;">
                <button class="pill-btn" data-cycle-btn="pno-cat"></button>
                <span class="lbl lbl-xs">INFO</span>
              </div>
              <div style="margin-top:6px;">
                ${makeKnob({ label: "MODEL", badge: `<span class="lbl-box">LIST</span>`, scale: "encoder", size: "enc" })}
              </div>
            </div>
          </div>
        </div>
      </section>
    `;
  }

  function buildProgramBay() {
    const slotTop = ["SYSTEM", "SOUND", "ORGANIZE", "AUX KB"];
    const slotBot = ["OUTPUT", "PEDAL", "MIDI", "EXTERN"];

    return `
      <section class="bay bay-red bay-program" id="bay-program">
        <!-- Top Row: MORPH ASSIGN, SPLIT, MST CLK, TRANSP -->
        <div class="prog-top-row">
          <div class="morph-box">
            <div class="morph-hdr">MORPH ASSIGN</div>
            <div class="morph-body">
              <div class="morph-grid">
                <span class="lbl-pill-white"><span class="led sm on green" data-led="m-whl"></span>WHEEL▾</span>
                <span class="lbl-pill-white"><span class="led sm" data-led="m-at"></span>A.T.▾</span>
                <span class="lbl-pill-white"><span class="led sm" data-led="m-ped"></span>CTRLPED▾</span>
                <button class="pill-btn sm" data-toggle-led="m-whl"></button>
                <button class="pill-btn sm" data-toggle-led="m-at"></button>
                <button class="pill-btn sm" data-toggle-led="m-ped"></button>
              </div>
              <span class="lbl lbl-xs" style="text-align:center;">CLEAR MORPH</span>
            </div>
          </div>

          <div class="prog-top-mini">
            <span class="lbl-pill-white">SPLIT</span>
            <div class="row" style="gap:2px;" data-split-leds="1"><span class="led sm on red"></span><span class="led sm"></span><span class="led sm"></span></div>
            <span class="lbl lbl-xs">ON/SET ▽</span>
            <button class="pill-btn sm" data-split-btn="1"></button>
            <span class="lbl lbl-xs">SET KEY</span>
          </div>

          <div class="prog-top-mini">
            <span class="lbl-pill-white">MST CLK</span>
            <span class="lbl lbl-xs">TAP/SET ▽</span>
            <button class="pill-btn sm" data-flash-led="mst-clk-led"></button>
            <div class="row"><span class="led sm" data-led="mst-clk-led"></span><span class="lbl lbl-xs">PEDAL TAP</span></div>
          </div>

          <div class="prog-top-mini">
            <span class="lbl-pill-white">TRANSP</span>
            <div class="row" style="gap:2px;"><span class="led sm" data-led="transp-top-led"></span><span class="lbl lbl-xs">ON/SET ▽</span></div>
            <button class="pill-btn sm" data-toggle-led="transp-top-led"></button>
            <span class="lbl lbl-xs">PANIC</span>
          </div>
        </div>

        <!-- Main 3-Column Program Area -->
        <div class="prog-main-area">
          <!-- Left Col: STORE, Dial, Page/Bank, Live, Layer Scene II -->
          <div class="prog-col-left">
            <div class="col" style="gap:2px;">
              <div class="row"><span class="led sm" data-led="store-led"></span><span class="lbl lbl-xs">STORE</span></div>
              <button class="pill-btn red" data-toggle-led="store-led"></button>
              <span class="lbl lbl-xs">STORE AS...</span>
              <span class="lbl lbl-xs lbl-muted">PAGE NAME</span>
            </div>

            <div class="row" style="gap:6px;">
              <div class="col" style="gap:2px;">
                <span class="lbl lbl-xs">MIDI</span>
                <span class="led sm" data-led="midi-led"></span>
              </div>
              <div class="col" style="gap:2px;">
                <span class="lbl lbl-xs">EXTERN</span>
                <span class="led sm" data-led="transp-led"></span>
              </div>
            </div>

            ${makeKnob({ label: "PROGRAM", badge: `<span class="lbl-box">LIST</span>`, scale: "encoder", size: "enc" })}

            <div class="col" style="gap:2px;">
              <span class="lbl lbl-xs">◀ PAGE/CAT ▶</span>
              <div class="row" style="gap:3px;">
                <button class="pill-btn sm" data-bank-step="-1"></button>
                <button class="pill-btn sm" data-bank-step="1"></button>
              </div>
              <span class="lbl lbl-xs">◀ BANK ▶</span>
            </div>

            <div class="col" style="gap:2px;">
              <div class="row" style="gap:3px;"><span class="led sm" data-led="live-led"></span><span class="lbl lbl-xs">LIVE MODE</span></div>
              <button class="pill-btn grey sm" data-toggle-led="live-led"></button>
              <div class="row" style="gap:3px;"><span class="led sm" data-led="numpad-led"></span><span class="lbl lbl-xs">NUM PAD</span></div>
            </div>

            <div class="col" style="gap:2px; background:#2c3440; border:1px solid rgba(255,255,255,0.35); border-radius:4px; padding:3px 5px; width:100%;">
              <div class="row" style="gap:3px;"><span class="led sm" data-led="scene2-led"></span><span class="lbl lbl-xs" style="text-align:left; line-height:1.0;">LAYER<br/>SCENE II</span></div>
              <button class="pill-btn sm" data-toggle-led="scene2-led"></button>
              <div class="row" style="gap:3px;"><span class="led sm" data-led="scene2-ped"></span><span class="lbl lbl-xs">PEDAL</span></div>
            </div>
          </div>

          <!-- Center Col: PRESET LIBRARY, MAIN OLED, PROGRAM 1..8 -->
          <div class="prog-col-mid">
            <div class="preset-lib-box">
              <div class="preset-lib-hdr">PRESET LIBRARY</div>
              <div class="row around" style="padding:3px 4px 1px;">
                <div class="row" style="gap:2px;"><span class="led sm" data-led="plib-org"></span><span class="lbl lbl-xs">ORGAN</span></div>
                <div class="row" style="gap:2px;"><span class="led sm" data-led="plib-pno"></span><span class="lbl lbl-xs">PIANO</span></div>
                <div class="row" style="gap:2px;"><span class="led sm" data-led="plib-syn"></span><span class="lbl lbl-xs">SYNTH</span></div>
              </div>
              <div class="row around" style="padding:2px 4px;">
                <button class="pill-btn sm" data-toggle-led="plib-org"></button>
                <button class="pill-btn sm" data-toggle-led="plib-pno"></button>
                <button class="pill-btn sm" data-toggle-led="plib-syn"></button>
              </div>
              <div class="lbl lbl-xs" style="text-align:center; padding-bottom:2px;">SINGLE LAYER</div>
            </div>

            <!-- MAIN OLED DISPLAY -->
            <div class="oled-bezel oled-main" id="main-oled">
              <div>
                <div class="prog-num" id="oled-prog-num">A:11</div>
                <div class="prog-name" id="oled-prog-name">nd matrix 1</div>
              </div>
              <div class="prog-layers">
                <div class="prog-layer-row"><span><span class="oled-tag">O</span>1:02 B3 Soulful</span><span></span></div>
                <div class="prog-layer-row"><span><span class="oled-tag">P</span>White Grand</span><span>XL</span></div>
                <div class="prog-layer-row"><span><span class="oled-tag">S</span>2:35 Vista Pad Whl</span><span></span></div>
              </div>
            </div>

            <!-- PROGRAM 1..8 BANK -->
            <div class="program-bank-box">
              <div class="preset-lib-hdr">PROGRAM</div>
              <div class="program-bank-grid">
                ${[1, 2, 3, 4].map((n, i) => `
                  <div class="prog-slot-cell">
                    <div class="row" style="gap:2px;"><span class="led sm ${n === 1 ? "on red" : ""}" data-prog-led="${n}"></span><span class="lbl lbl-xs">${n}</span></div>
                    <button class="pill-btn sm" data-prog-btn="${n}"></button>
                    <span class="lbl" style="font-size:5.8px;">${slotTop[i]}</span>
                  </div>
                `).join("")}
                ${[5, 6, 7, 8].map((n, i) => `
                  <div class="prog-slot-cell">
                    <div class="row" style="gap:2px;"><span class="led sm" data-prog-led="${n}"></span><span class="lbl lbl-xs">${n}</span></div>
                    <button class="pill-btn sm" data-prog-btn="${n}"></button>
                    <span class="lbl" style="font-size:5.8px;">${slotBot[i]}</span>
                  </div>
                `).join("")}
              </div>
            </div>
          </div>

          <!-- Right Col: PROG VIEW, SOLO/UNDO, SECTION EDIT, MON/COPY, SHIFT/EXIT -->
          <div class="prog-col-right">
            <div class="col" style="gap:2px;">
              <span class="lbl lbl-xs">PROG VIEW</span>
              <button class="pill-btn sm"></button>
              <span class="lbl" style="font-size:6px;">PRESET NAME</span>
            </div>

            <div class="col" style="gap:5px; border:1px solid rgba(255,255,255,0.45); border-radius:4px; padding:4px 3px; width:100%;">
              <div class="col" style="gap:2px;">
                <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">SOLO</span></div>
                <button class="pill-btn sm"></button>
                <span class="lbl lbl-xs">UNDO</span>
              </div>
              <div class="col" style="gap:2px;">
                <div class="row" style="gap:2px;"><span class="led sm" data-led="secedit-led"></span><span class="lbl" style="font-size:6px; text-align:left;">SECTION<br/>EDIT ‡</span></div>
                <button class="pill-btn sm" data-toggle-led="secedit-led"></button>
                <span class="lbl" style="font-size:6px;">LAYER INIT</span>
              </div>
              <div class="col" style="gap:2px;">
                <span class="lbl" style="font-size:6px;">MON/COPY</span>
                <button class="pill-btn sm"></button>
                <span class="lbl" style="font-size:6px;">PASTE ‡</span>
              </div>
            </div>

            <div class="shift-exit-box">
              <span class="lbl lbl-dark lbl-xs">SHIFT</span>
              <button class="pill-btn grey vert"></button>
              <span class="lbl lbl-dark lbl-xs">EXIT</span>
            </div>
          </div>
        </div>
      </section>
    `;
  }

  function buildSynthBay() {
    return `
      <section class="bay bay-slate bay-synth" id="bay-synth">
        <div class="bay-header has-notch">
          ${makeHeaderStepSvg(658, 396, "syn")}
          <div class="bay-header-title">
            <span class="h-main">SYNTH</span>
            <span class="h-sub">SECTION</span>
          </div>
          <div class="header-fx-focus" style="margin-left:54px;">
            <span class="lbl lbl-dark lbl-xs">FX FOCUS</span>
            <span class="led on amber" data-led="syn-fx-focus"></span>
          </div>
          <div class="header-on-solo">
            <div class="header-on-col">
              <span class="lbl lbl-dark lbl-xs">ON</span>
              <span class="led on red" data-led="syn-on"></span>
            </div>
            <button class="pill-btn" data-toggle-led="syn-on"></button>
            <span class="lbl lbl-dark lbl-xs">SOLO ▾</span>
          </div>
          <div class="header-red-notch" style="width: 396px;"></div>
        </div>

        <div class="synth-body">
          <!-- Left: 3 Faders A/B/C + KB HOLD / ARP RUN + Octave -->
          <div class="synth-left-col">
            <div class="faders-row" style="gap:8px;">
              ${makeFaderStrip(0.82)}
              ${makeFaderStrip(0.32)}
              ${makeFaderStrip(0.75)}
            </div>

            <div class="layer-btns-row" style="gap:3px;">
              <div class="layer-btn-col">
                <div class="row" style="gap:1px;"><span class="lbl lbl-xs">A</span><span class="lbl" style="font-size:5.8px;">AUX KB</span><span class="led sm"></span></div>
                <span class="lbl-pill-white" style="padding:1px 2px; font-size:5.8px;"><span class="led sm on amber" data-led="syn-a"></span>ON/OFF▾</span>
                <button class="pill-btn grey sm" data-toggle-led="syn-a"></button>
                <div class="row"><span class="led sm on red"></span><span class="lbl" style="font-size:5.8px;">SUSTPED</span></div>
              </div>
              <div class="layer-btn-col">
                <div class="row" style="gap:1px;"><span class="lbl lbl-xs">B</span><span class="lbl" style="font-size:5.8px;">AUX KB</span><span class="led sm"></span></div>
                <span class="lbl-pill-white" style="padding:1px 2px; font-size:5.8px;"><span class="led sm on amber" data-led="syn-b"></span>ON/OFF▾</span>
                <button class="pill-btn grey sm" data-toggle-led="syn-b"></button>
                <div class="row"><span class="led sm on red"></span><span class="lbl" style="font-size:5.5px;">PSTICK/RNG▽</span></div>
              </div>
              <div class="layer-btn-col">
                <div class="row" style="gap:1px;"><span class="lbl lbl-xs">C</span><span class="lbl" style="font-size:5.8px;">AUX KB</span><span class="led sm"></span></div>
                <span class="lbl-pill-white" style="padding:1px 2px; font-size:5.8px;"><span class="led sm on amber" data-led="syn-c"></span>ON/OFF▾</span>
                <button class="pill-btn grey sm" data-toggle-led="syn-c"></button>
                <span class="lbl" style="font-size:5.8px;">PAN ▾</span>
              </div>
            </div>

            <div class="row around w-full">
              <div class="col" style="gap:2px;">
                <span class="lbl-pill-red"><span class="led sm" data-led="kb-hold"></span>KB HOLD</span>
                <button class="pill-btn sm" data-toggle-led="kb-hold"></button>
                <div class="row"><span class="led sm"></span><span class="lbl" style="font-size:6px;">EXCLUDE ▽</span></div>
              </div>
              <div class="col" style="gap:2px;">
                <div class="row"><span class="led sm" data-led="arp-run"></span><span class="lbl lbl-xs">ARP RUN</span></div>
                <button class="pill-btn red sm" data-toggle-led="arp-run"></button>
                <div class="row"><span class="led sm"></span><span class="lbl" style="font-size:6px;">KB SYNC ▽</span></div>
              </div>
            </div>

            <div class="octave-zone-block">
              <span class="lbl lbl-xs">◀ OCTAVE SHIFT ▶</span>
              <div class="row" style="gap:3px;">
                <button class="pill-btn sm" data-zone-shift="syn,-1"></button>
                <button class="pill-btn sm" data-zone-shift="syn,1"></button>
              </div>
              <span class="lbl lbl-xs">◀ KB ZONE ▶</span>
              <div class="row" style="gap:4px;" data-zone-leds="syn">
                <span class="led sm"></span>
                <span class="led sm"></span>
                <span class="led sm on green"></span>
                <span class="led sm on green"></span>
              </div>
            </div>
          </div>

          <!-- Right: Top (OLED + MODE + ARP/VOICE/VIB) & Bottom (LFO / OSC / FILTER / AMP+UNISON) -->
          <div class="synth-main-col">
            <div class="synth-top-row">
              <!-- Synth OLED + 3 Encoders -->
              <div class="synth-oled-col">
                <div class="oled-bezel oled-synth">
                  <div class="oled-synth-bar">OSC WAVEFORM</div>
                  <div class="oled-synth-mid">
                    <div>
                      <div class="oled-synth-wave-name">Super Saw</div>
                      <div class="oled-synth-detune">DETUNE: 3.4</div>
                    </div>
                    <svg width="42" height="24" viewBox="0 0 42 24">
                      <polyline points="2,20 10,8 10,20 18,8 18,20 26,8 26,20" fill="none" stroke="#9cc7eb" stroke-width="1.2"/>
                      <polyline points="6,17 14,5 14,17 22,5 22,17 30,5 30,17" fill="none" stroke="#d9ecff" stroke-width="1.2"/>
                      <polyline points="10,14 18,2 18,14 26,2 26,14 34,2 34,14" fill="none" stroke="#eef7ff" stroke-width="1.2"/>
                    </svg>
                  </div>
                  <div class="oled-synth-tabs">
                    <div class="oled-soft-tab"><span>ANALOG</span><span class="tab-pill">TYPE</span></div>
                    <div class="oled-soft-tab"><span>SUPER</span><span class="tab-pill">CAT</span></div>
                    <div class="oled-soft-tab"><span>1 (5)</span><span class="tab-pill">WAVE</span></div>
                  </div>
                </div>

                <!-- Pointer lines from OLED tabs to encoders -->
                <svg class="synth-pointer-svg" viewBox="0 0 146 36">
                  <polyline points="26,0 26,10 14,22 14,36" fill="none" stroke="#aeb9c5" stroke-width="1.2"/>
                  <line x1="73" y1="0" x2="73" y2="36" stroke="#aeb9c5" stroke-width="1.2"/>
                  <polyline points="120,0 120,10 132,22 132,36" fill="none" stroke="#aeb9c5" stroke-width="1.2"/>
                </svg>

                <div class="synth-encoders-row">
                  ${makeKnob({ label: "INFO", scale: "encoder", size: "enc" })}
                  ${makeKnob({ label: "LIST", scale: "encoder", size: "enc" })}
                  ${makeKnob({ label: "LIST", scale: "encoder", size: "enc" })}
                </div>
              </div>

              <!-- MODE + WAVEFORM -->
              <div class="synth-mode-col">
                <div class="silver-subpanel col between w-full" style="padding:4px 4px 4px; height:86px;">
                  <span class="lbl lbl-dark">MODE</span>
                  <div class="col" style="align-items:flex-start; gap:3px;">
                    <div class="row"><span class="led sm" data-led="syn-mode-smp"></span><span class="lbl lbl-dark" style="font-size:6.5px;">SAMPLES</span></div>
                    <div class="row"><span class="led sm on red" data-led="syn-mode-ana"></span><span class="lbl lbl-dark" style="font-size:6.5px;">ANALOG</span></div>
                  </div>
                  <button class="pill-btn sm" data-toggle-pair="syn-mode-ana,syn-mode-smp"></button>
                  <div class="row"><span class="led sm"></span><span class="lbl lbl-dark" style="font-size:6.5px;">EXTERN</span></div>
                </div>

                <div class="col" style="gap:3px;">
                  <span class="lbl lbl-xs">WAVEFORM</span>
                  <div class="row"><span class="led sm on red" data-led="syn-keep"></span><span class="lbl" style="font-size:5.8px;">KEEP EDITS▽</span></div>
                  <button class="pill-btn vert red-frame" data-toggle-led="syn-keep"></button>
                  <span class="lbl lbl-xs" style="text-align:center;">SOUND<br/>INIT</span>
                </div>
              </div>

              <!-- ARPEGGIATOR / GATE + VOICE + VIBRATO -->
              <div class="synth-arp-voice-col">
                <div class="group-box" style="flex:1; display:flex; align-items:center; justify-content:space-between; padding:8px 6px 4px;">
                  <span class="group-box-title">ARPEGGIATOR / GATE</span>
                  ${makeKnob({
                    label: "RATE/TIME",
                    scale: "0-10",
                    value: 0.62,
                    size: "sm",
                    morphLed: true,
                    extraBottom: `<span class="lbl-pill-red mt-2"><span class="led sm on red"></span>MST CLK</span>`
                  })}
                  <div class="col" style="gap:4px;">
                    <div class="row" style="gap:4px;">
                      <div class="col" style="align-items:flex-end; gap:3px;">
                        <div class="row"><span class="lbl lbl-xs">POLY</span><span class="led sm"></span></div>
                        <div class="row"><span class="lbl lbl-xs">ARP</span><span class="led sm on red" data-led="arp-mode"></span></div>
                      </div>
                      <span class="lbl-pill-blue">GATE</span>
                    </div>
                    <button class="pill-btn sm" data-toggle-led="arp-mode"></button>
                    <span class="lbl lbl-xs">PATTERN ▽</span>
                  </div>
                  ${makeKnob({
                    label: "RANGE",
                    badge: `<span class="lbl-box">ENV</span>`,
                    scale: "range",
                    value: 0.5,
                    size: "sm",
                    morphLed: true
                  })}
                  <div class="col" style="gap:3px;">
                    <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">MENU</span></div>
                    <button class="pill-btn vert red-frame"></button>
                    <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">GROUP▽</span></div>
                  </div>
                </div>

                <div class="row" style="gap:5px; height:76px;">
                  <div class="group-box row between" style="flex:1.25; height:100%; padding:8px 6px 4px;">
                    <span class="group-box-title">VOICE</span>
                    <div class="col between" style="height:100%;">
                      <div class="col" style="align-items:flex-start; gap:3px;">
                        <div class="row"><span class="led sm" data-led="v-mono"></span><span class="lbl lbl-xs">MONO</span></div>
                        <div class="row"><span class="led sm" data-led="v-leg"></span><span class="lbl lbl-xs">LEGATO</span></div>
                      </div>
                      <button class="pill-btn sm" data-cycle-leds="v-mono,v-leg" data-allow-off="1"></button>
                      <div class="row"><span class="led sm"></span><span class="lbl lbl-xs">LO▽</span><span class="led sm"></span><span class="lbl lbl-xs">HI▽</span></div>
                    </div>
                    ${makeKnob({ label: "GLIDE", scale: "0-10", value: 0.65, size: "sm", morphLed: true })}
                  </div>

                  <div class="group-box row between" style="flex:1; height:100%; padding:8px 6px 4px;">
                    <span class="group-box-title">VIBRATO</span>
                    <div class="col between" style="height:100%;">
                      <div class="row" style="gap:4px;">
                        <div class="col" style="align-items:flex-end; gap:2px;">
                          <div class="row"><span class="lbl lbl-xs">WHL</span><span class="led sm" data-led="vib-whl"></span></div>
                          <div class="row"><span class="lbl lbl-xs">DLY</span><span class="led sm" data-led="vib-dly"></span></div>
                          <div class="row"><span class="lbl lbl-xs">ON</span><span class="led sm" data-led="vib-on"></span></div>
                        </div>
                        <div class="col" style="align-items:flex-start; gap:4px;">
                          <span class="lbl lbl-xs">A.T.</span>
                          <span class="lbl lbl-xs">PED</span>
                        </div>
                      </div>
                      <button class="pill-btn sm" data-cycle-leds="vib-whl,vib-dly,vib-on" data-allow-off="1"></button>
                    </div>
                    <div class="col" style="gap:3px;">
                      <div class="row"><span class="led sm" data-led="vib-menu"></span><span class="lbl lbl-xs">MENU</span></div>
                      <button class="pill-btn vert red-frame" data-toggle-led="vib-menu"></button>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <!-- Bottom Row: LFO, OSCILLATORS, FILTER, AMP + UNISON -->
            <div class="synth-bottom-row">
              <!-- LFO -->
              <div class="silver-subpanel synth-sub-box">
                <span class="synth-sub-title">LFO</span>
                <div class="row between w-full synth-sub-top">
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="led sm" data-led="lfo-wf"></span><span class="lbl lbl-dark" style="font-size:6.2px;">WAVEFORM</span></div>
                    <button class="pill-btn sm red-frame" data-toggle-led="lfo-wf"></button>
                    <div class="row"><span class="led sm"></span><span class="lbl lbl-dark" style="font-size:6px;">GROUP ▽</span></div>
                  </div>
                  ${makeKnob({ label: "MOD AMT", scale: "0-10", value: 0.58, size: "sm", morphLed: true })}
                </div>
                <div class="row between w-full">
                  ${makeKnob({
                    label: "RATE/TIME",
                    scale: "0-10",
                    value: 0.25,
                    size: "sm",
                    morphLed: true,
                    extraBottom: `<span class="lbl-pill-red mt-2"><span class="led sm"></span>MST CLK</span>`
                  })}
                  <div class="col" style="gap:3px; align-items:flex-end;">
                    <div class="row" style="gap:4px;">
                      <div class="col" style="align-items:flex-end; gap:2px;">
                        <div class="row"><span class="lbl lbl-dark" style="font-size:5.8px;">OSC PITCH</span><span class="led sm" data-led="lfo-d1"></span></div>
                        <div class="row"><span class="lbl lbl-dark" style="font-size:5.8px;">OSC CTRL</span><span class="led sm" data-led="lfo-d2"></span></div>
                      </div>
                      <span class="lbl lbl-dark" style="font-size:5.8px;">FILTER</span>
                    </div>
                    <button class="pill-btn sm" data-cycle-3="lfo-d1,lfo-d2"></button>
                  </div>
                </div>
              </div>

              <!-- OSCILLATORS -->
              <div class="silver-subpanel synth-sub-box">
                <span class="synth-sub-title">OSCILLATORS</span>
                <div class="row around w-full synth-sub-top">
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="led sm" data-led="osc-p"></span><span class="lbl lbl-dark" style="font-size:6px;">PITCH/SMP</span></div>
                    <button class="pill-btn sm red-frame" data-toggle-led="osc-p"></button>
                    <div class="row"><span class="led sm"></span><span class="lbl lbl-dark" style="font-size:5.5px;">ENV TO PITCH▽</span></div>
                  </div>
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="led sm" data-led="osc-e"></span><span class="lbl lbl-dark" style="font-size:6px;">ENVELOPE</span></div>
                    <button class="pill-btn sm red-frame" data-toggle-led="osc-e"></button>
                    <div class="row"><span class="led sm"></span><span class="lbl lbl-dark" style="font-size:5.5px;">VELOCITY ▽</span></div>
                  </div>
                </div>
                <div class="row around w-full">
                  ${makeKnob({ label: "OSC CTRL", scale: "0-10", value: 0.35, size: "sm", darkInlay: true, morphLed: true })}
                  ${makeKnob({ label: "ENV AMT", scale: "bipolar10", value: 0.62, size: "sm", morphLed: true })}
                </div>
              </div>

              <!-- FILTER -->
              <div class="silver-subpanel synth-sub-box">
                <span class="synth-sub-title">FILTER</span>
                <div class="row between w-full synth-sub-top">
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="led sm" data-led="flt-t"></span><span class="lbl lbl-dark" style="font-size:6px;">TYPE</span></div>
                    <button class="pill-btn sm red-frame" data-toggle-led="flt-t"></button>
                    <div class="row"><span class="led sm"></span><span class="lbl lbl-dark" style="font-size:5.8px;">GROUP ▽</span></div>
                  </div>
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="led sm" data-led="flt-e"></span><span class="lbl lbl-dark" style="font-size:6px;">ENVELOPE</span></div>
                    <button class="pill-btn sm red-frame" data-toggle-led="flt-e"></button>
                    <div class="row"><span class="led sm"></span><span class="lbl lbl-dark" style="font-size:5.8px;">VELOCITY ▽</span></div>
                  </div>
                  ${makeKnob({ label: "ENV AMT", scale: "0-10", value: 0.45, size: "sm", morphLed: true })}
                </div>
                <div class="row between w-full">
                  ${makeKnob({ label: "FREQ", scale: "0-10", value: 0.38, size: "sm", darkInlay: true, morphLed: true })}
                  ${makeKnob({ label: "RES/FREQ HP", scale: "0-10", value: 0.52, size: "sm", darkInlay: true, morphLed: true })}
                  <div class="col" style="gap:2px;">
                    <div class="row"><span class="led sm on red" data-led="flt-on"></span><span class="lbl lbl-dark" style="font-size:5.8px;">FILTER<br/>ON</span></div>
                    <button class="pill-btn vert" data-toggle-led="flt-on"></button>
                  </div>
                </div>
              </div>

              <!-- AMP + UNISON -->
              <div class="col between" style="height:100%; gap:10px;">
                <div class="silver-subpanel col w-full" style="padding:13px 3px 4px; flex:1; justify-content:center; gap:3px;">
                  <span class="synth-sub-title">AMP</span>
                  <div class="row" style="gap:2px;"><span class="led sm" data-led="amp-env-led"></span><span class="lbl lbl-dark" style="font-size:5.5px;">ENVELOPE</span></div>
                  <button class="pill-btn sm red-frame" data-cycle-3="amp-v1,amp-v2"></button>
                  <span class="lbl lbl-dark" style="font-size:5.5px;">VELOCITY▽</span>
                  <div class="row" style="gap:2px;"><span class="lbl lbl-dark" style="font-size:5.5px;">1</span><span class="led sm" data-led="amp-v1"></span><span class="lbl lbl-dark" style="font-size:5.5px;">2</span><span class="led sm on red" data-led="amp-v2"></span></div>
                </div>
                <div class="group-box col w-full" style="padding:9px 3px 5px; height:54px; justify-content:center; gap:5px;">
                  <span class="group-box-title">UNISON</span>
                  <div class="row" style="gap:3px;">
                    <div class="col" style="gap:2px;">
                      <div class="row"><span class="lbl lbl-xs">2</span><span class="led sm on red" data-led="syn-uni-2"></span></div>
                      <div class="row"><span class="lbl lbl-xs">1</span><span class="led sm" data-led="syn-uni-1"></span></div>
                    </div>
                    <span class="lbl lbl-xs">3</span>
                  </div>
                  <button class="pill-btn sm" data-cycle-3="syn-uni-1,syn-uni-2"></button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>
    `;
  }

  function buildFxBay() {
    return `
      <section class="bay bay-fx" id="bay-fx">
        <div class="bay-header has-notch">
          ${makeHeaderStepSvg(582, 320, "fx")}
          <div class="bay-header-title">
            <span class="h-main">LAYER EFFECTS</span>
            <span class="h-sub">FX FOCUS</span>
          </div>
          <div class="header-on-solo" style="margin-left:18px;">
            <div class="header-on-col">
              <span class="lbl lbl-dark lbl-xs">ON</span>
              <span class="led on red" data-led="fx-master-on"></span>
            </div>
            <button class="pill-btn" data-toggle-led="fx-master-on"></button>
          </div>
          <div class="header-red-notch" style="width: 320px;"></div>
        </div>

        <div class="fx-wrapper">
          <!-- Left Red FX FOCUS Strip -->
          <div class="fx-focus-strip">
                  <div class="col" style="gap:3px;">
                    <span class="lbl lbl-xs">ORGAN</span>
                    <span class="led sm" data-led="fxf-org-a"></span>
                    <div class="row" style="gap:4px;"><span class="lbl lbl-xs">A</span><span class="lbl lbl-xs">B</span></div>
                    <button class="pill-btn sm" data-fx-focus="organ"></button>
                    <span class="lbl" style="font-size:6px;">ALL FX OFF</span>
                  </div>

                  <div class="col" style="gap:3px;">
                    <span class="lbl lbl-xs">PIANO</span>
                    <div class="row" style="gap:6px;"><span class="led sm" data-led="fxf-pno-a"></span><span class="led sm" data-led="fxf-pno-b"></span></div>
                    <div class="row" style="gap:6px;"><span class="lbl lbl-xs">A</span><span class="lbl lbl-xs">B</span></div>
                    <button class="pill-btn sm" data-fx-focus="piano"></button>
                    <span class="lbl lbl-xs">GROUP ▽</span>
                  </div>

                  <div class="col" style="gap:3px;">
                    <span class="lbl lbl-xs">SYNTH</span>
                    <div class="row" style="gap:5px;"><span class="led sm on amber" data-led="fxf-syn-a"></span><span class="led sm on amber" data-led="fxf-syn-b"></span><span class="led sm on amber" data-led="fxf-syn-c"></span></div>
                    <div class="row" style="gap:6px;"><span class="lbl lbl-xs">A</span><span class="lbl lbl-xs">B</span><span class="lbl lbl-xs">C</span></div>
                    <button class="pill-btn sm" data-fx-focus="synth"></button>
                    <span class="lbl lbl-xs">GROUP ▽</span>
                  </div>

            <div class="shift-exit-box" style="width:50px;">
              <span class="lbl lbl-dark lbl-xs">SHIFT</span>
              <button class="pill-btn vert"></button>
              <span class="lbl lbl-dark lbl-xs">EXIT</span>
            </div>
          </div>

          <!-- Right Slate FX Modules -->
          <div class="fx-slate-area">
            <!-- Left Column: MOD 1, MOD 2, AMP SIM / EQ -->
            <div class="fx-col-left">
              <!-- MOD 1 -->
              <div class="group-box row between" style="height:92px;">
                <span class="group-box-title">MOD 1</span>
                ${makeKnob({
                  label: "RATE",
                  badge: `<span class="lbl-box">SENS</span>`,
                  scale: "0-10",
                  value: 0.22,
                  size: "sm",
                  morphLed: true,
                  extraBottom: `<span class="lbl-pill-red mt-2"><span class="led sm on red"></span>MST CLK</span>`
                })}
                ${makeKnob({ label: "AMOUNT", scale: "0-10", value: 0.62, size: "sm", morphLed: true })}
                <div class="col" style="gap:3px;">
                  ${makeTriMatrix(["RM", "TREM", "A-PAN"], ["<span class='lbl-box'>A-WAH</span>", "<span class='lbl-pill-blue'>WAH</span>", "<span class='lbl-pill-blue'>PUMP</span>"], 5, "fx-mod1")}
                  <button class="pill-btn sm" data-cycle-btn="fx-mod1"></button>
                  <div class="row" style="gap:2px;"><span class="led sm"></span><span class="lbl" style="font-size:6px;">VARIATION <span class="lbl-pill-blue">PED</span>▽</span></div>
                </div>
                <div class="col" style="gap:3px;">
                  <div class="row"><span class="lbl lbl-xs">ON</span><span class="led sm on red" data-led="mod1-on"></span></div>
                  <button class="pill-btn grey vert" data-toggle-led="mod1-on"></button>
                </div>
              </div>

              <!-- MOD 2 -->
              <div class="group-box row between" style="height:86px;">
                <span class="group-box-title">MOD 2</span>
                ${makeKnob({ label: "RATE", scale: "0-10", value: 0.60, size: "sm", morphLed: true })}
                ${makeKnob({ label: "AMOUNT", scale: "0-10", value: 0.22, size: "sm", morphLed: true })}
                <div class="col" style="gap:3px;">
                  ${makeTriMatrix(["CHOR", "FLANG", "PHAS"], ["VIBE", "ENS", "SPIN"], 0, "fx-mod2")}
                  <button class="pill-btn sm" data-cycle-btn="fx-mod2"></button>
                  <div class="row"><span class="led sm on red"></span><span class="lbl" style="font-size:6px;">VARIATION ▽</span></div>
                </div>
                <div class="col" style="gap:3px;">
                  <div class="row"><span class="lbl lbl-xs">ON</span><span class="led sm on red" data-led="mod2-on"></span></div>
                  <button class="pill-btn grey vert" data-toggle-led="mod2-on"></button>
                </div>
              </div>

              <!-- AMP SIM / EQ -->
              <div class="group-box col between" style="flex:1; padding:8px 5px 4px;">
                <span class="group-box-title">AMP SIM / EQ</span>
                <div class="row between w-full">
                  <div class="col" style="align-items:flex-start;">
                    <div class="row"><span class="lbl lbl-xs">ON</span><span class="led sm on red" data-led="amp-drive-on"></span></div>
                    ${makeKnob({ label: "DRIVE", scale: "0-10", value: 0.25, size: "sm", morphLed: true })}
                  </div>
                  ${makeKnob({ label: "FREQ", badge: `<span class="lbl-pill-blue">FREQ</span>`, scale: "freq", value: 0.54, size: "md" })}
                  <div class="col" style="gap:3px;">
                    ${makeTriMatrix(["SMALL", "JC", "TWIN"], ["<span class='lbl-pill-red'>TO ROTARY</span>", "<span class='lbl-pill-blue'>LP FILTER</span>", "<span class='lbl-pill-blue'>HP FILTER</span>"], 2, "fx-amp")}
                    <button class="pill-btn sm" data-cycle-btn="fx-amp"></button>
                    <div class="row" style="gap:2px;"><span class="led sm"></span><span class="lbl" style="font-size:6px;">VARIATION ▽</span></div>
                  </div>
                </div>

                <div class="row between w-full">
                  ${makeKnob({ label: "BASS", scale: "bipolar15", value: 0.62, size: "sm" })}
                  ${makeKnob({ label: "MID", badge: `<span class="lbl-pill-blue">RES</span>`, scale: "bipolar15", value: 0.65, size: "sm" })}
                  ${makeKnob({ label: "TREBLE", scale: "bipolar15", value: 0.5, size: "sm" })}
                  <div class="col" style="gap:3px;">
                    <div class="row"><span class="lbl lbl-xs">ON</span><span class="led sm on red" data-led="amp-on"></span></div>
                    <button class="pill-btn grey vert" data-toggle-led="amp-on"></button>
                  </div>
                </div>
              </div>
            </div>

            <!-- Right Column: DELAY + (COMP | REVERB) -->
            <div class="fx-col-right">
              <!-- DELAY -->
              <div class="group-box col between" style="height:186px; padding:7px 5px 5px;">
                <span class="group-box-title">DELAY</span>
                <div class="row between w-full">
                  ${makeKnob({
                    label: "TEMPO",
                    scale: "0-10",
                    value: 0.28,
                    size: "sm",
                    morphLed: true,
                    extraBottom: `<span class="lbl-pill-red mt-2"><span class="led sm on red"></span>MST CLK</span>`
                  })}
                  <div class="col" style="gap:2px;">
                    <span class="lbl lbl-xs">EFFECTS</span>
                    <div class="row" style="gap:3px;">
                      <div class="col" style="align-items:flex-end; gap:2px;">
                        <div class="row"><span class="lbl" style="font-size:6px;">CHOR</span><span class="led sm" data-led="dly-fx1"></span></div>
                        <div class="row"><span class="lbl" style="font-size:6px;">VIBE</span><span class="led sm on red" data-led="dly-fx2"></span></div>
                        <div class="row"><span class="lbl" style="font-size:6px;">ENS</span><span class="led sm on red" data-led="dly-fx3"></span></div>
                      </div>
                      <div class="col" style="align-items:flex-start; gap:3px;">
                        <span class="lbl" style="font-size:6px;">FLAM</span>
                        <span class="lbl" style="font-size:6px;">SPACE</span>
                      </div>
                    </div>
                    <button class="pill-btn sm" data-cycle-leds="dly-fx1,dly-fx2,dly-fx3" data-allow-off="1"></button>
                    <div class="row" style="gap:2px;"><span class="led sm"></span><span class="lbl" style="font-size:6px;">VARIATION ▽</span></div>
                  </div>
                  ${makeKnob({ label: "FEEDBACK", scale: "0-10", value: 0.62, size: "sm", morphLed: true })}
                </div>

                <div class="row between w-full">
                  <div class="silver-subpanel col" style="padding:3px 4px; gap:3px;">
                    <div class="row"><span class="led sm" data-led="dly-tap"></span><span class="lbl lbl-dark" style="font-size:6px;">TAP/SET▾</span></div>
                    <button class="pill-btn sm" data-flash-led="dly-tap"></button>
                    <div class="row"><span class="led sm" data-led="dly-ana"></span><span class="lbl lbl-dark" style="font-size:6px;">ANALOG▽</span></div>
                  </div>
                  ${makeKnob({ label: "DRY  WET", scale: "0-10", value: 0.25, size: "sm", morphLed: true })}
                  <div class="col" style="gap:3px;">
                    <span class="lbl lbl-xs">FILTER</span>
                    <div class="row" style="gap:3px;">
                      <div class="col" style="align-items:flex-end; gap:2px;">
                        <div class="row"><span class="lbl" style="font-size:6px;">HP</span><span class="led sm" data-led="dly-hp"></span></div>
                        <div class="row"><span class="lbl" style="font-size:6px;">LP</span><span class="led sm" data-led="dly-lp"></span></div>
                      </div>
                      <span class="lbl" style="font-size:6px;">BP</span>
                    </div>
                    <button class="pill-btn sm" data-cycle-3="dly-hp,dly-lp"></button>
                    <div class="row" style="gap:2px;"><span class="lbl" style="font-size:6px;">PING PONG▽</span><span class="led sm"></span></div>
                  </div>
                </div>

                <div class="row" style="justify-content:flex-end; width:100%; gap:4px;">
                  <div class="col" style="gap:1px;"><span class="lbl lbl-xs">ON</span><span class="led sm on red" data-led="dly-on"></span></div>
                  <div class="col" style="gap:2px;">
                    <button class="pill-btn grey sm" data-toggle-led="dly-on"></button>
                    <span class="lbl-pill-red"><span class="led sm"></span>GLOBAL▽</span>
                  </div>
                </div>
              </div>

              <!-- COMP + REVERB -->
              <div class="fx-bottom-right">
                <div class="group-box col between" style="width:66px; padding:8px 3px 4px;">
                  <span class="group-box-title">COMP</span>
                  <div class="row"><span class="led sm"></span><span class="lbl" style="font-size:6px;">ACTIVE</span></div>
                  ${makeKnob({ label: "AMOUNT", scale: "0-10", value: 0.22, size: "sm" })}
                  <div class="row"><span class="led sm"></span><span class="lbl" style="font-size:6px;">FAST</span></div>
                  <div class="row" style="gap:3px;">
                    <div class="col" style="gap:1px;"><span class="lbl" style="font-size:6px;">ON</span><span class="led sm" data-led="comp-on"></span></div>
                    <div class="col" style="gap:2px;">
                      <button class="pill-btn grey sm" data-toggle-led="comp-on"></button>
                      <span class="lbl-pill-red"><span class="led sm"></span>GLOBAL▽</span>
                    </div>
                  </div>
                </div>

                <div class="group-box col between" style="flex:1; padding:8px 4px 4px;">
                  <span class="group-box-title">REVERB</span>
                  <div class="row between w-full">
                    <div class="col" style="gap:3px;">
                      <div class="row"><span class="led sm" data-led="rev-brt"></span><span class="lbl" style="font-size:6px;">BRIGHT</span></div>
                      <div class="row"><span class="led sm" data-led="rev-drk"></span><span class="lbl" style="font-size:6px;">DARK</span></div>
                      <button class="pill-btn sm" data-cycle-leds="rev-brt,rev-drk" data-allow-off="1"></button>
                    </div>
                    <div class="col" style="gap:2px;">
                      ${makeTriMatrix(["ROOM", "BOOTH", "SPRING"], ["STAGE", "HALL", "CATH"], 5, "fx-rev")}
                      <button class="pill-btn sm" data-cycle-btn="fx-rev"></button>
                      <div class="row" style="gap:2px;"><span class="lbl" style="font-size:5.8px;">VAR|CHORALE▽</span><span class="led sm on red"></span></div>
                    </div>
                  </div>
                  <div class="row between w-full">
                    ${makeKnob({ label: "DRY  WET", scale: "0-10", value: 0.62, size: "sm", morphLed: true })}
                    <div class="row" style="gap:3px;">
                      <div class="col" style="gap:1px;"><span class="lbl lbl-xs">ON</span><span class="led sm on red" data-led="rev-on"></span></div>
                      <div class="col" style="gap:2px;">
                        <button class="pill-btn grey sm" data-toggle-led="rev-on"></button>
                        <span class="lbl-pill-red"><span class="led sm"></span>GLOBAL▽</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>
    `;
  }

  // ------------------------------------------------------------------------
  // 88-Key Hammer Action Keybed (A0..C8: 52 white keys + 36 black keys)
  // ------------------------------------------------------------------------

  function buildKeybed() {
    const container = document.getElementById("keybed-keys");
    const whiteNotes = [];
    // MIDI 21 (A0) to 108 (C8)
    const isBlack = (midi) => [1, 3, 6, 8, 10].includes(midi % 12);
    const blacks = [];
    let whiteIdx = 0;

    for (let midi = 21; midi <= 108; midi++) {
      if (isBlack(midi)) {
        blacks.push({ midi, leftWhiteCount: whiteIdx });
      } else {
        whiteNotes.push({ midi, idx: whiteIdx });
        whiteIdx++;
      }
    }

    let html = "";
    for (const w of whiteNotes) {
      html += `<div class="key-white" data-midi="${w.midi}"></div>`;
    }
    for (const b of blacks) {
      const leftPct = (b.leftWhiteCount / whiteNotes.length) * 100;
      html += `<div class="key-black" data-midi="${b.midi}" style="left: ${leftPct.toFixed(4)}%;"></div>`;
    }
    container.innerHTML = html;
  }

  // ------------------------------------------------------------------------
  // Interactive Controls Wiring
  // ------------------------------------------------------------------------

  function wireInteractions() {
    // 1. Toggle single LED buttons
    document.querySelectorAll("[data-toggle-led]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.getAttribute("data-toggle-led");
        const led = document.querySelector(`[data-led="${id}"]`);
        if (led) {
          const hasColor = led.classList.contains("green") || led.classList.contains("amber") || led.classList.contains("red");
          if (!hasColor) led.classList.add("red");
          led.classList.toggle("on");
        }
      });
    });

    // 2. Toggle LED pair (e.g. SLOW/FAST, ANALOG/SAMPLES)
    document.querySelectorAll("[data-toggle-pair]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const [aId, bId] = btn.getAttribute("data-toggle-pair").split(",");
        const a = document.querySelector(`[data-led="${aId}"]`);
        const b = document.querySelector(`[data-led="${bId}"]`);
        if (a && b) {
          const aOn = a.classList.contains("on");
          a.classList.toggle("on", !aOn);
          b.classList.toggle("on", aOn);
          if (!b.classList.contains("green") && !b.classList.contains("amber")) b.classList.add("red");
        }
      });
    });

    // 3. Cycle triangular LED matrices
    document.querySelectorAll("[data-cycle-btn]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const key = btn.getAttribute("data-cycle-btn");
        const group = document.querySelector(`[data-cycle-group="${key}"]`);
        if (!group) return;
        const count = Number(group.getAttribute("data-cycle-count")) || 6;
        const next = ((Number(group.getAttribute("data-cycle-index")) || 0) + 1) % count;
        group.setAttribute("data-cycle-index", String(next));
        group.querySelectorAll("[data-tri-idx]").forEach((tri) => {
          tri.classList.toggle("on", Number(tri.getAttribute("data-tri-idx")) === next);
        });
      });
    });

    // 4. Program 1..8 buttons update Main OLED
    const progPresets = {
      1: { num: "A:11", name: "nd matrix 1" },
      2: { num: "A:12", name: "Royal Grand 3D" },
      3: { num: "A:13", name: "B3 Gospel Swell" },
      4: { num: "A:14", name: "MK1 Suitcase EP" },
      5: { num: "A:15", name: "Super Saw Lead" },
      6: { num: "A:16", name: "Horizon Whl" },
      7: { num: "A:17", name: "Warm Analog Pad" },
      8: { num: "A:18", name: "Stage Split Live" }
    };
    document.querySelectorAll("[data-prog-btn]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const n = Number(btn.getAttribute("data-prog-btn"));
        document.querySelectorAll("[data-prog-led]").forEach((l) => {
          l.classList.toggle("on", Number(l.getAttribute("data-prog-led")) === n);
          l.classList.add("red");
        });
        const p = progPresets[n];
        if (p) {
          document.getElementById("oled-prog-num").textContent = p.num;
          document.getElementById("oled-prog-name").textContent = p.name;
        }
      });
    });

    // 5. Draggable Rotary Knobs
    document.querySelectorAll("[data-knob]").forEach((knob) => {
      const isEncoder = knob.getAttribute("data-encoder") === "1";
      let dragging = false;
      let startY = 0;
      let startVal = 0.5;

      knob.addEventListener("pointerdown", (e) => {
        dragging = true;
        startY = e.clientY;
        startVal = Number(knob.getAttribute("data-value")) || 0.5;
        knob.setPointerCapture(e.pointerId);
      });

      knob.addEventListener("pointermove", (e) => {
        if (!dragging) return;
        const delta = (startY - e.clientY) / 140;
        let val = isEncoder ? startVal + delta : Math.max(0, Math.min(1, startVal + delta));
        knob.setAttribute("data-value", String(val));
        const angle = isEncoder ? val * 360 : -135 + val * 270;
        knob.style.transform = `rotate(${angle}deg)`;
      });

      knob.addEventListener("pointerup", () => { dragging = false; });
    });

    // 6. Draggable Layer Level Faders (live 11-LED green ladder)
    document.querySelectorAll("[data-fader-strip]").forEach((strip) => {
      const track = strip.querySelector("[data-fader-track]");
      const cap = strip.querySelector(".fader-cap");
      const segs = strip.querySelectorAll(".led-seg");
      let dragging = false;

      const updateFromEvent = (e) => {
        const rect = track.getBoundingClientRect();
        const t = Math.max(0, Math.min(1, 1 - (e.clientY - rect.top) / rect.height));
        strip.setAttribute("data-value", t.toFixed(3));
        cap.style.top = `${10 + (1 - t) * 80}%`;
        const lit = Math.round(t * 11);
        segs.forEach((s, i) => s.classList.toggle("on", i < lit) || (i < lit && s.classList.add("green")));
        segs.forEach((s, i) => {
          s.classList.toggle("on", i < lit);
          s.classList.toggle("green", i < lit);
        });
      };

      track.addEventListener("pointerdown", (e) => {
        dragging = true;
        track.setPointerCapture(e.pointerId);
        updateFromEvent(e);
      });
      track.addEventListener("pointermove", (e) => { if (dragging) updateFromEvent(e); });
      track.addEventListener("pointerup", () => { dragging = false; });
    });

    // 7. Draggable Organ Drawbars (live 8-LED red ladder)
    document.querySelectorAll("[data-drawbar]").forEach((unit) => {
      const slot = unit.querySelector("[data-drawbar-slot]");
      const cap = unit.querySelector(".drawbar-cap");
      const segs = unit.querySelectorAll(".led-seg");
      let dragging = false;

      const updateFromEvent = (e) => {
        const rect = slot.getBoundingClientRect();
        const raw = Math.max(0, Math.min(1, (e.clientY - rect.top - 20) / 84));
        const step = Math.round(raw * 8);
        unit.setAttribute("data-value", String(step));
        cap.style.top = `${6 + (step / 8) * 78}px`;
        segs.forEach((s, i) => {
          const on = i < step;
          s.classList.toggle("on", on);
          s.classList.toggle("red", on);
        });
      };

      slot.addEventListener("pointerdown", (e) => {
        dragging = true;
        slot.setPointerCapture(e.pointerId);
        updateFromEvent(e);
      });
      slot.addEventListener("pointermove", (e) => { if (dragging) updateFromEvent(e); });
      slot.addEventListener("pointerup", () => { dragging = false; });
    });

    // 8. Pitch Stick (spring return) & Mod Wheel (smooth stone drum roll)
    const pitchWell = document.getElementById("pitch-stick");
    const pitchHandle = document.getElementById("pitch-stick-handle");
    let pitchDrag = false;
    let pitchStartX = 0;
    pitchWell.addEventListener("pointerdown", (e) => {
      pitchDrag = true;
      pitchStartX = e.clientX;
      pitchWell.setPointerCapture(e.pointerId);
    });
    pitchWell.addEventListener("pointermove", (e) => {
      if (!pitchDrag) return;
      const dx = Math.max(-12, Math.min(12, e.clientX - pitchStartX));
      pitchHandle.style.transform = `translateX(${dx}px)`;
    });
    pitchWell.addEventListener("pointerup", () => {
      pitchDrag = false;
      pitchHandle.style.transform = "translateX(0px)";
    });

    const modWell = document.getElementById("mod-wheel");
    const modDrum = document.getElementById("mod-wheel-drum");
    let modDrag = false;
    let modStartY = 0;
    let modVal = 0; // 0 (bottom) .. 1 (top)
    const renderModWheel = () => {
      const notchTop = 74 - modVal * 62; // 74% at bottom -> 12% at top
      modDrum.style.setProperty("--mod-notch-top", `${notchTop}%`);
      modDrum.style.backgroundPosition = `0 0, 0 ${(-modVal * 36).toFixed(1)}px, 0 0`;
      modWell.title = `Mod Wheel: ${Math.round(modVal * 127)}`;
    };
    renderModWheel();
    modWell.addEventListener("pointerdown", (e) => {
      modDrag = true;
      modStartY = e.clientY;
      modWell.setPointerCapture(e.pointerId);
    });
    modWell.addEventListener("pointermove", (e) => {
      if (!modDrag) return;
      const dy = (modStartY - e.clientY) / 90;
      modStartY = e.clientY;
      modVal = Math.max(0, Math.min(1, modVal + dy));
      renderModWheel();
    });
    modWell.addEventListener("pointerup", () => { modDrag = false; });

    // 8b. Multi-LED cyclers, 3-state (1/2/3) cyclers, Split, Bank, FX Focus, Octave/Zone shift
    document.querySelectorAll("[data-cycle-leds]").forEach((btn) => {
      const ids = btn.getAttribute("data-cycle-leds").split(",");
      const allowOff = btn.getAttribute("data-allow-off") === "1";
      btn.addEventListener("click", () => {
        const leds = ids.map((id) => document.querySelector(`[data-led="${id}"]`)).filter(Boolean);
        let cur = leds.findIndex((l) => l.classList.contains("on"));
        let next = cur + 1;
        if (next >= leds.length) next = allowOff ? -1 : 0;
        leds.forEach((l, i) => {
          l.classList.add("red");
          l.classList.toggle("on", i === next);
        });
      });
    });

    document.querySelectorAll("[data-cycle-3]").forEach((btn) => {
      const [id1, id2] = btn.getAttribute("data-cycle-3").split(",");
      let state = 0; // 0=off, 1=led1, 2=led2, 3=both
      btn.addEventListener("click", () => {
        state = (state + 1) % 4;
        const l1 = document.querySelector(`[data-led="${id1}"]`);
        const l2 = document.querySelector(`[data-led="${id2}"]`);
        if (l1) { l1.classList.add("red"); l1.classList.toggle("on", state === 1 || state === 3); }
        if (l2) { l2.classList.add("red"); l2.classList.toggle("on", state === 2 || state === 3); }
      });
    });

    document.querySelectorAll("[data-flash-led]").forEach((btn) => {
      const id = btn.getAttribute("data-flash-led");
      btn.addEventListener("click", () => {
        const l = document.querySelector(`[data-led="${id}"]`);
        if (!l) return;
        l.classList.add("on", "red");
        setTimeout(() => l.classList.remove("on"), 220);
      });
    });

    let splitCount = 1;
    document.querySelectorAll("[data-split-btn]").forEach((btn) => {
      btn.addEventListener("click", () => {
        splitCount = (splitCount + 1) % 4;
        const leds = document.querySelectorAll("[data-split-leds] .led");
        leds.forEach((l, i) => { l.classList.add("red"); l.classList.toggle("on", i < splitCount); });
        document.querySelectorAll(".split-led").forEach((sl, i) => {
          sl.style.opacity = i < splitCount ? "1" : "0.18";
        });
      });
    });

    const banks = ["A", "B", "C", "D"];
    let bankIdx = 0;
    document.querySelectorAll("[data-bank-step]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const step = Number(btn.getAttribute("data-bank-step"));
        bankIdx = (bankIdx + step + banks.length) % banks.length;
        const el = document.getElementById("oled-prog-num");
        if (el) el.textContent = `${banks[bankIdx]}:${el.textContent.split(":")[1] || "11"}`;
      });
    });

    document.querySelectorAll("[data-zone-shift]").forEach((btn) => {
      const [sec, dirStr] = btn.getAttribute("data-zone-shift").split(",");
      const dir = Number(dirStr);
      btn.addEventListener("click", () => {
        const leds = Array.from(document.querySelectorAll(`[data-zone-leds="${sec}"] .led`));
        if (!leds.length) return;
        const states = leds.map((l) => l.classList.contains("on"));
        if (dir > 0) states.unshift(states.pop());
        else states.push(states.shift());
        leds.forEach((l, i) => { l.classList.add("green"); l.classList.toggle("on", states[i]); });
      });
    });

    document.querySelectorAll("[data-fx-focus]").forEach((btn) => {
      const target = btn.getAttribute("data-fx-focus");
      btn.addEventListener("click", () => {
        const groups = {
          organ: { hdr: "org-fx-focus", strip: ["fxf-org-a", "fxf-org-b"], color: "red" },
          piano: { hdr: "pno-fx-focus", strip: ["fxf-pno-a", "fxf-pno-b"], color: "green" },
          synth: { hdr: "syn-fx-focus", strip: ["fxf-syn-a", "fxf-syn-b", "fxf-syn-c"], color: "amber" }
        };
        Object.entries(groups).forEach(([k, g]) => {
          const active = k === target;
          const h = document.querySelector(`[data-led="${g.hdr}"]`);
          if (h) { h.classList.add(g.color); h.classList.toggle("on", active); }
          g.strip.forEach((id) => {
            const l = document.querySelector(`[data-led="${id}"]`);
            if (l) { l.classList.add(g.color); l.classList.toggle("on", active); }
          });
        });
      });
    });

    // 9. Keybed press animation
    const keybed = document.getElementById("keybed-keys");
    let keyDown = false;
    keybed.addEventListener("pointerdown", (e) => {
      const k = e.target.closest("[data-midi]");
      if (!k) return;
      keyDown = true;
      k.classList.add("active");
    });
    window.addEventListener("pointerup", () => {
      keyDown = false;
      keybed.querySelectorAll(".active").forEach((k) => k.classList.remove("active"));
    });
    keybed.addEventListener("pointerover", (e) => {
      if (!keyDown) return;
      const k = e.target.closest("[data-midi]");
      if (k) {
        keybed.querySelectorAll(".active").forEach((el) => el.classList.remove("active"));
        k.classList.add("active");
      }
    });
  }

  // ------------------------------------------------------------------------
  // Viewport Zoom & Section Focus Inspector
  // ------------------------------------------------------------------------

  let currentZoom = 0.65;

  function applyZoom(z) {
    currentZoom = Math.max(0.3, Math.min(2.0, z));
    const chassis = document.getElementById("ns4-chassis");
    const bounds = document.getElementById("stage-bounds");
    const scaler = document.getElementById("stage-scaler");
    scaler.style.transform = `scale(${currentZoom})`;
    bounds.style.width = `${Math.ceil(chassis.offsetWidth * currentZoom)}px`;
    bounds.style.height = `${Math.ceil(chassis.offsetHeight * currentZoom)}px`;
    document.getElementById("zoom-readout").textContent = `${Math.round(currentZoom * 100)}%`;
  }

  function fitToViewport() {
    const vp = document.getElementById("viewport");
    const chassis = document.getElementById("ns4-chassis");
    const pad = 44;
    const scaleX = (vp.clientWidth - pad) / chassis.offsetWidth;
    const scaleY = (vp.clientHeight - pad) / chassis.offsetHeight;
    applyZoom(Math.min(scaleX, scaleY, 1.0));
  }

  function focusSection(section) {
    const vp = document.getElementById("viewport");
    if (section === "full") {
      fitToViewport();
      return;
    }
    const map = {
      organ: "bay-organ",
      piano: "bay-piano",
      program: "bay-program",
      synth: "bay-synth",
      fx: "bay-fx"
    };
    const el = document.getElementById(map[section]);
    if (!el) return;
    applyZoom(1.25);
    requestAnimationFrame(() => {
      const chassisRect = document.getElementById("ns4-chassis").getBoundingClientRect();
      const elRect = el.getBoundingClientRect();
      const targetLeft = (elRect.left - chassisRect.left) + (elRect.width / 2) - (vp.clientWidth / 2) + 20;
      vp.scrollTo({ left: Math.max(0, targetLeft), top: 0, behavior: "instant" });
    });
  }

  // ------------------------------------------------------------------------
  // Boot
  // ------------------------------------------------------------------------

  function buildGroupBoxBorders() {
    document.querySelectorAll(".group-box").forEach((box) => {
      const old = box.querySelector(".group-box-border-svg");
      if (old) old.remove();
      const w = box.offsetWidth;
      const h = box.offsetHeight;
      if (!w || !h) return;
      const r = 5;
      const title = box.querySelector(".group-box-title");
      const cx = w / 2;
      const gapHalf = title ? Math.min(w / 2 - r - 2, title.offsetWidth / 2 + 2) : 0;
      const x1 = Math.max(r, cx - gapHalf);
      const x2 = Math.min(w - r, cx + gapHalf);
      const d = title
        ? `M ${x2.toFixed(1)} 0.5 L ${(w - r).toFixed(1)} 0.5 A ${r} ${r} 0 0 1 ${(w - 0.5).toFixed(1)} ${r} L ${(w - 0.5).toFixed(1)} ${(h - r).toFixed(1)} A ${r} ${r} 0 0 1 ${(w - r).toFixed(1)} ${(h - 0.5).toFixed(1)} L ${r} ${(h - 0.5).toFixed(1)} A ${r} ${r} 0 0 1 0.5 ${(h - r).toFixed(1)} L 0.5 ${r} A ${r} ${r} 0 0 1 ${r} 0.5 L ${x1.toFixed(1)} 0.5`
        : `M ${r} 0.5 L ${(w - r).toFixed(1)} 0.5 A ${r} ${r} 0 0 1 ${(w - 0.5).toFixed(1)} ${r} L ${(w - 0.5).toFixed(1)} ${(h - r).toFixed(1)} A ${r} ${r} 0 0 1 ${(w - r).toFixed(1)} ${(h - 0.5).toFixed(1)} L ${r} ${(h - 0.5).toFixed(1)} A ${r} ${r} 0 0 1 0.5 ${(h - r).toFixed(1)} L 0.5 ${r} A ${r} ${r} 0 0 1 ${r} 0.5 Z`;
      const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      svg.setAttribute("class", "group-box-border-svg");
      svg.setAttribute("viewBox", `0 0 ${w} ${h}`);
      svg.setAttribute("aria-hidden", "true");
      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", d);
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", "#9da9b5");
      path.setAttribute("stroke-width", "1");
      svg.appendChild(path);
      box.insertBefore(svg, box.firstChild);
    });
  }

  function init() {
    const bays = document.getElementById("panel-bays");
    bays.innerHTML =
      buildMasterBay() +
      buildOrganBay() +
      buildPianoBay() +
      buildProgramBay() +
      buildSynthBay() +
      buildFxBay();

    buildKeybed();
    wireInteractions();
    requestAnimationFrame(buildGroupBoxBorders);

    document.getElementById("btn-fit").addEventListener("click", fitToViewport);
    document.getElementById("btn-zoom-in").addEventListener("click", () => applyZoom(currentZoom + 0.12));
    document.getElementById("btn-zoom-out").addEventListener("click", () => applyZoom(currentZoom - 0.12));
    document.getElementById("btn-toggle-keys").addEventListener("click", (e) => {
      const kb = document.getElementById("keybed-frame");
      kb.classList.toggle("hidden");
      e.target.classList.toggle("active", !kb.classList.contains("hidden"));
      fitToViewport();
    });

    document.querySelectorAll("#focus-tabs .tb-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.querySelectorAll("#focus-tabs .tb-btn").forEach((b) => b.classList.remove("active"));
        btn.classList.add("active");
        focusSection(btn.getAttribute("data-focus"));
      });
    });

    window.addEventListener("resize", () => {
      const active = document.querySelector("#focus-tabs .tb-btn.active");
      if (active && active.getAttribute("data-focus") === "full") fitToViewport();
    });

    fitToViewport();
  }

  // ------------------------------------------------------------------------
  // Baker Hooks (called by tools/bake_panel_from_html.py)
  // ------------------------------------------------------------------------
  window.__ns4ExtractLayout = function () {
    const chassis = document.getElementById("ns4-chassis");
    const cRect = chassis.getBoundingClientRect();
    const norm = (el) => {
      const r = el.getBoundingClientRect();
      return {
        x: Number(((r.left - cRect.left) / cRect.width).toFixed(6)),
        y: Number(((r.top - cRect.top) / cRect.height).toFixed(6)),
        w: Number((r.width / cRect.width).toFixed(6)),
        h: Number((r.height / cRect.height).toFixed(6)),
        px: [
          Math.round(r.left - cRect.left),
          Math.round(r.top - cRect.top),
          Math.round(r.width),
          Math.round(r.height)
        ]
      };
    };

    const bays = {};
    document.querySelectorAll("#panel-bays > section").forEach((b) => {
      bays[b.id] = norm(b);
    });

    const knobs = Array.from(document.querySelectorAll("[data-knob]")).map((el, i) => ({
      index: i,
      encoder: el.getAttribute("data-encoder") === "1",
      ...norm(el)
    }));
    const buttons = Array.from(document.querySelectorAll(".pill-btn")).map((el, i) => ({
      index: i,
      variant: el.classList.contains("grey")
        ? "grey"
        : el.classList.contains("red")
          ? "red"
          : "black",
      vertical: el.classList.contains("vert"),
      ...norm(el)
    }));
    const faders = Array.from(document.querySelectorAll("[data-fader-track]")).map((el, i) => ({
      index: i,
      ...norm(el)
    }));
    const drawbars = Array.from(document.querySelectorAll("[data-drawbar-slot]")).map((el, i) => ({
      index: i,
      ...norm(el)
    }));
    const leds = Array.from(document.querySelectorAll(".led, .split-led")).map((el, i) => ({
      index: i,
      id: el.getAttribute("data-led") || null,
      split: el.classList.contains("split-led"),
      ...norm(el)
    }));
    const oleds = {
      main: norm(document.getElementById("main-oled")),
      synth: norm(document.querySelector(".oled-synth"))
    };
    const wheels = {
      pitch_stick: norm(document.getElementById("pitch-stick")),
      mod_wheel: norm(document.getElementById("mod-wheel"))
    };
    const keys = Array.from(document.querySelectorAll("#keybed-keys [data-midi]")).map((el) => ({
      midi: Number(el.getAttribute("data-midi")),
      black: el.classList.contains("key-black"),
      ...norm(el)
    }));

    return {
      chassis_px: { width: Math.round(cRect.width), height: Math.round(cRect.height) },
      bays,
      knobs,
      buttons,
      faders,
      drawbars,
      leds,
      oleds,
      wheels,
      keys
    };
  };

  window.__ns4StageSprite = function (name) {
    document.body.classList.remove("bake-static");
    document.body.classList.add("bake-sprite-stage");
    let stage = document.getElementById("sprite-stage");
    if (!stage) {
      stage = document.createElement("div");
      stage.id = "sprite-stage";
      stage.className = "sprite-stage";
      document.body.appendChild(stage);
    }
    const templates = {
      btn_black_up: `<button class="pill-btn"></button>`,
      btn_black_down: `<button class="pill-btn pressed"></button>`,
      btn_grey_up: `<button class="pill-btn grey"></button>`,
      btn_grey_down: `<button class="pill-btn grey pressed"></button>`,
      btn_red_up: `<button class="pill-btn red"></button>`,
      btn_red_down: `<button class="pill-btn red pressed"></button>`,
      knob_pot_cap: `<div class="knob-body" style="transform:rotate(0deg);"><div class="knob-pointer"></div></div>`,
      knob_encoder_cap: `<div class="knob-body encoder"></div>`,
      fader_cap: `<div style="position:relative;width:30px;height:24px;"><div class="fader-cap" style="top:50%;"></div></div>`,
      drawbar_white: `<div style="position:relative;width:28px;height:70px;"><div class="drawbar-cap white" style="top:4px;"></div></div>`,
      drawbar_black: `<div style="position:relative;width:28px;height:70px;"><div class="drawbar-cap black" style="top:4px;"></div></div>`,
      pitch_stick: `<div class="pitch-stick-wood"></div>`,
      mod_wheel_drum: `<div class="mod-wheel-drum"></div>`
    };
    stage.innerHTML = templates[name] || "";
    const target = stage.firstElementChild;
    const r = target.getBoundingClientRect();
    const pad = 14;
    return {
      x: Math.floor(r.left) - pad,
      y: Math.floor(r.top) - pad,
      width: Math.ceil(r.width) + pad * 2,
      height: Math.ceil(r.height) + pad * 2
    };
  };

  init();
})();
