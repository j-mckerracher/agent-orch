/*
MIT License

Copyright (c) 2026 Builder.io, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

MIT License

Copyright (c) 2025 Vercel, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

Bundled from the public Agent-Native renderer asset renderer-BGT6fWD2.js,
inspected 2026-11-08. Vite network preloading was replaced with a local loader.

Local change for MCAO (M7-18): the only edit is the next declaration, `var` became
`export const`, so the bundle loads as an ES module (`import()`) under the CSP
`script-src 'self'` instead of as a classic script that assigns a global.
*/
export const AgentNativeOcean = (() => {
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __typeError = (msg) => {
    throw TypeError(msg);
  };
  var __defNormalProp = (obj, key, value) => key in obj ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value }) : obj[key] = value;
  var __require = /* @__PURE__ */ ((x2) => typeof require !== "undefined" ? require : typeof Proxy !== "undefined" ? new Proxy(x2, {
    get: (a2, b2) => (typeof require !== "undefined" ? require : a2)[b2]
  }) : x2)(function(x2) {
    if (typeof require !== "undefined") return require.apply(this, arguments);
    throw Error('Dynamic require of "' + x2 + '" is not supported');
  });
  var __esm = (fn2, res, err) => function __init() {
    if (err) throw err[0];
    try {
      return fn2 && (res = (0, fn2[__getOwnPropNames(fn2)[0]])(fn2 = 0)), res;
    } catch (e) {
      throw err = [e], e;
    }
  };
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to2, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to2, key) && key !== except)
          __defProp(to2, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to2;
  };
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);
  var __publicField = (obj, key, value) => __defNormalProp(obj, typeof key !== "symbol" ? key + "" : key, value);
  var __accessCheck = (obj, member, msg) => member.has(obj) || __typeError("Cannot " + msg);
  var __privateIn = (member, obj) => Object(obj) !== obj ? __typeError('Cannot use the "in" operator on this value') : member.has(obj);
  var __privateGet = (obj, member, getter) => (__accessCheck(obj, member, "read from private field"), getter ? getter.call(obj) : member.get(obj));
  var __privateAdd = (obj, member, value) => member.has(obj) ? __typeError("Cannot add the same private member more than once") : member instanceof WeakSet ? member.add(obj) : member.set(obj, value);
  var __privateSet = (obj, member, value, setter) => (__accessCheck(obj, member, "write to private field"), setter ? setter.call(obj, value) : member.set(obj, value), value);
  var __privateMethod = (obj, member, method) => (__accessCheck(obj, member, "access private method"), method);

  // ../../../../private/tmp/agent-wave-build/rolldown-runtime-C6BX6AHi.js
  var t2, s, u;
  var init_rolldown_runtime_C6BX6AHi = __esm({
    "../../../../private/tmp/agent-wave-build/rolldown-runtime-C6BX6AHi.js"() {
      (function() {
        try {
          var e = typeof window < `u` ? window : typeof global < `u` ? global : typeof globalThis < `u` ? globalThis : typeof self < `u` ? self : {}, t4 = new e.Error().stack;
          t4 && (e._sentryDebugIds = e._sentryDebugIds || {}, e._sentryDebugIds[t4] = `fc68cdeb-04c1-4af1-9aa2-9325216ad402`, e._sentryDebugIdIdentifier = `sentry-dbid-fc68cdeb-04c1-4af1-9aa2-9325216ad402`);
        } catch {
        }
      })();
      t2 = Object.defineProperty;
      s = (e, n2) => {
        let r2 = {};
        for (var i2 in e) t2(r2, i2, { get: e[i2], enumerable: true });
        return n2 || t2(r2, Symbol.toStringTag, { value: `Module` }), r2;
      };
      u = ((e) => typeof __require < `u` ? __require : typeof Proxy < `u` ? new Proxy(e, { get: (e2, t4) => (typeof __require < `u` ? __require : e2)[t4] }) : e)(function(e) {
        if (typeof __require < `u`) return __require.apply(this, arguments);
        throw Error('Calling `require` for "' + e + "\" in an environment that doesn't expose the `require` function. See https://rolldown.rs/in-depth/bundling-cjs#require-external-modules for more details.");
      });
    }
  });

  // ../../../../private/tmp/agent-wave-build/dist-CW40cPHt.js
  var dist_CW40cPHt_exports = {};
  __export(dist_CW40cPHt_exports, {
    $: () => it,
    A: () => De,
    B: () => Pe,
    C: () => lt,
    D: () => Ae,
    E: () => dt,
    F: () => Re,
    G: () => ft,
    H: () => qe,
    I: () => ze,
    J: () => rt,
    K: () => Ee,
    L: () => Fe,
    M: () => me,
    N: () => Me,
    O: () => pt,
    P: () => Ie,
    Q: () => $e,
    R: () => Le,
    S: () => ct,
    T: () => Ce,
    U: () => Ue,
    V: () => Ne,
    W: () => He,
    X: () => nt,
    Y: () => et,
    Z: () => tt,
    _: () => Se,
    a: () => ye,
    at: () => we,
    b: () => ke,
    c: () => _e,
    ct: () => be,
    d: () => ot,
    dt: () => g,
    et: () => We,
    f: () => xe,
    ft: () => h,
    g: () => Oe,
    h: () => Xe,
    i: () => Y,
    it: () => Be,
    j: () => pe,
    k: () => Ke,
    l: () => he,
    lt: () => U,
    m: () => Je,
    n: () => _t,
    nt: () => Ze,
    o: () => ve,
    ot: () => X,
    p: () => Ye,
    q: () => de,
    r: () => gt,
    rt: () => fe,
    s: () => ge,
    st: () => Ve,
    t: () => Ct,
    tt: () => Qe,
    u: () => at,
    ut: () => _,
    v: () => Te,
    w: () => st,
    x: () => ut,
    y: () => ue,
    z: () => je
  });
  function r(e) {
    return new t3({ code: `VGPU-FEATURE-UNSUPPORTED`, message: `Adapter does not support requested feature(s): ${e.map((e2) => `"${e2}"`).join(`, `)}.`, fix: `Remove the unsupported name(s) from init({ requiredFeatures: [...] }) or run on an adapter that supports them; gate optional code paths on device.features after init.`, where: `init` });
  }
  function i(e, t4) {
    if (!e) return;
    let n2 = (t4 ?? []).filter((t5) => !e.has(t5));
    if (n2.length) throw r(n2);
  }
  function o(e) {
    let t4 = globalThis.GPUBufferUsage;
    return e.reduce((e2, n2) => e2 | s2(n2, t4), 0);
  }
  function s2(e, t4) {
    let n2 = e.toUpperCase();
    return t4?.[n2] ?? a[e];
  }
  function c() {
    return globalThis.GPUMapMode?.READ ?? 1;
  }
  function u2(e) {
    let t4 = globalThis.GPUTextureUsage;
    return e.reduce((e2, n2) => e2 | d(n2, t4), 0);
  }
  function d(e, t4) {
    let n2 = e.toUpperCase();
    return t4?.[n2] ?? l[e];
  }
  function f(e) {
    return `__vgpuMockBytes` in e;
  }
  function p(e) {
    return `__vgpuMockBytes` in e;
  }
  function h(e) {
    return Object.freeze({ kind: e, id: m++ });
  }
  function v(e, t4, r2) {
    return new n({ code: `VGPU-EXTERNAL-BUFFER-VALIDATION`, message: t4, where: e, cause: r2, fix: `Use a buffer with the required usage flags and an aligned in-range operation.` });
  }
  function y(e) {
    if (S(e)) throw ee();
    let t4 = { version: 1, mappings: [] }, n2 = { version: 1, modules: [{ path: `<runtime>`, text: e }], diagnostics: [], sourceMap: t4, cacheKey: b(e) };
    return { kind: `wgsl`, wgsl: e, source: { text: e, path: `<runtime>`, imports: [] }, ast: n2, sourceMap: t4, diagnostics: [], cacheKey: n2.cacheKey, entryPoints: x(e), stats: { lines: e.split(/\r?\n/).length, bytes: new TextEncoder().encode(e).byteLength, bindGroups: 0 } };
  }
  function b(e) {
    let t4 = 2166136261;
    for (let n2 = 0; n2 < e.length; n2++) t4 = Math.imul(t4 ^ e.charCodeAt(n2), 16777619);
    return { default: `vgpu-wgsl-1:${(t4 >>> 0).toString(16).padStart(8, `0`)}` };
  }
  function x(e) {
    let t4 = [];
    for (let n2 of e.matchAll(/@(vertex|fragment|compute)\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)/g)) t4.push(n2[2]);
    return t4;
  }
  function S(e) {
    let t4 = e.replace(/\/\*[\s\S]*?\*\//g, ``).replace(/\/\/.*$/gm, ``).trimStart();
    return t4.startsWith(`import `) || t4.startsWith(`import{`);
  }
  function ee() {
    let e = Error(`Runtime WGSL strings cannot contain import statements. Use a build-time loader or @vgpu/wgsl/runtime.`);
    return e.name = `VGPUWGSLRuntimeImportError`, e.code = `VGPU-WGSL-RUNTIME-IMPORT`, e.severity = `error`, e.source = `wgsl`, e;
  }
  function te(e, t4) {
    let n2 = 2 ** t4;
    return [Math.max(1, Math.floor(e.width / n2)), Math.max(1, Math.floor(e.height / n2)), e.dimension === `3d` ? Math.max(1, Math.floor(e.depthOrArrayLayers / n2)) : e.depthOrArrayLayers];
  }
  function C(e, t4, r2) {
    let i2 = (e2) => {
      throw new n({ code: `VGPU-CORE-TEXTURE-READ-INVALID`, where: r2, message: e2, fix: `Select an allocated mip and an in-bounds region on a single-sample texture with usage "copy_src".` });
    };
    (!t4 || !Number.isSafeInteger(t4.mipLevel) || t4.mipLevel < 0 || t4.mipLevel >= e.mipLevelCount) && i2(`mipLevel must select an allocated mip using a nonnegative safe integer.`), e.sampleCount !== 1 && i2(`Texture readback requires sampleCount 1; resolve multisampled textures explicitly before reading.`), e.usage & u2([`copy_src`]) || i2(`Texture readback requires explicit "copy_src" usage.`);
    let a2 = te(e, t4.mipLevel), o2 = t4.region === `all` ? [0, 0, 0] : t4.region?.origin, s3 = t4.region === `all` ? a2 : t4.region?.size;
    (!Array.isArray(o2) || o2.length !== 3 || !Array.isArray(s3) || s3.length !== 3) && i2(`region must be "all" or contain origin and size triples.`);
    for (let e2 = 0; e2 < 3; e2++) {
      let n2 = o2[e2], r3 = s3[e2];
      (!Number.isSafeInteger(n2) || n2 < 0 || !Number.isSafeInteger(r3) || r3 <= 0 || n2 > a2[e2] || r3 > a2[e2] - n2) && i2(`Read region axis ${e2} is outside mip ${t4.mipLevel} extent [${a2}].`);
    }
    return { mipLevel: t4.mipLevel, origin: [...o2], size: [...s3], extent: a2 };
  }
  function w(e, t4, r2) {
    let i2 = Math.min(t4 ?? 4294967295, 4294967295);
    if (!Number.isSafeInteger(e) || e <= 0 || e > i2) throw new n({ code: `VGPU-CORE-TEXTURE-READ-INVALID`, where: r2, message: `Readback allocation of ${e} bytes exceeds the supported limit (${i2}).`, fix: `Read a smaller region or a higher mip level.` });
  }
  function D(e) {
    try {
      e.unmap();
    } catch {
    }
  }
  function O(e) {
    try {
      e.destroy();
    } catch {
    }
  }
  function ne(e, t4) {
    return Math.ceil(e / t4) * t4;
  }
  function A(e, t4) {
    let r2 = k[e];
    if (r2) return r2;
    throw new n({ code: `VGPU-CORE-UNSUPPORTED-FORMAT`, message: `Texture.read does not support format ${e}. Supported formats: ${Object.keys(k).join(`, `)}.`, where: t4 });
  }
  function re(e, t4, n2 = `Texture.readFloats`) {
    let r2 = A(t4, n2), i2 = r2.bytesPerPixel / r2.components, a2 = Math.floor(e.byteLength / i2), o2 = new Float32Array(a2), s3 = new DataView(e.buffer, e.byteOffset, e.byteLength);
    for (let e2 = 0; e2 < a2; e2++) r2.componentType === `unorm8` ? o2[e2] = s3.getUint8(e2) / 255 : r2.componentType === `float16` ? o2[e2] = ie(s3.getUint16(e2 * 2, true)) : o2[e2] = s3.getFloat32(e2 * 4, true);
    return o2;
  }
  function ie(e) {
    let t4 = e & 32768 ? -1 : 1, n2 = e >> 10 & 31, r2 = e & 1023;
    return n2 === 0 ? t4 * r2 * 2 ** -24 : n2 === 31 ? r2 === 0 ? t4 * (1 / 0) : NaN : t4 * (r2 + 1024) * 2 ** (n2 - 25);
  }
  function ae(e, t4, n2) {
    let { size: [r2, i2, a2], origin: [o2, s3, c2], extent: l2 } = t4, u3 = r2 * n2.bytesPerPixel, d2 = new Uint8Array(u3 * i2 * a2);
    for (let t5 = 0; t5 < a2; t5++) for (let r3 = 0; r3 < i2; r3++) {
      let a3 = (((c2 + t5) * l2[1] + s3 + r3) * l2[0] + o2) * n2.bytesPerPixel;
      d2.set(e.subarray(a3, a3 + u3), (t5 * i2 + r3) * u3);
    }
    return n2.swizzle === `bgra-to-rgba` && j(d2), d2;
  }
  function j(e) {
    for (let t4 = 0; t4 < e.length; t4 += 4) {
      let n2 = e[t4];
      e[t4] = e[t4 + 2], e[t4 + 2] = n2;
    }
  }
  function M(e) {
    return { size: e, usage: o([`copy_src`, `copy_dst`]) };
  }
  function R(e) {
    return Object.freeze({ ...e, size: Object.freeze([...e.size]), usage: Object.freeze([...e.usage]), ...e.viewFormats === void 0 ? {} : { viewFormats: Object.freeze([...e.viewFormats]) } });
  }
  function z(e) {
    return [e.size[0], e.kind === `1d` ? 1 : e.size[1], e.kind === `2d-array` ? e.layers : e.kind === `3d` ? e.size[2] : 1];
  }
  function B(e, t4) {
    let r2 = (e2, t5, r3) => {
      throw new n({ code: e2, message: t5, fix: r3, where: `Device.createTexture` });
    };
    (!e || ![`1d`, `2d`, `3d`, `2d-array`].includes(e.kind)) && r2(`VGPU-TEXTURE-KIND-REQUIRED`, `Texture kind must be explicit.`, `Pass kind: "1d", "2d", "3d", or "2d-array".`), `dimension` in e && r2(`VGPU-TEXTURE-KIND-REQUIRED`, `Texture dimension has been replaced by kind.`, `Use kind and, for "2d-array", a separate layers count.`);
    let i2 = e.kind === `1d` ? 1 : e.kind === `3d` ? 3 : 2;
    (!Array.isArray(e.size) || e.size.length !== i2 || !Array.from(e.size).every(V)) && r2(`VGPU-TEXTURE-SIZE-REQUIRED`, `Texture ${e.kind} needs ${i2} positive integer spatial dimensions.`, `Pass size with exactly the spatial dimensions of kind; put array layers in layers.`), (e.kind === `2d-array` ? !V(e.layers) : e.layers !== void 0) && r2(`VGPU-TEXTURE-LAYERS-INVALID`, `Only 2d-array textures take a positive integer layers count.`, `Use kind: "2d-array", size: [width, height], layers: count.`), (!Array.isArray(e.usage) || e.usage.length === 0 || !Array.from(e.usage).every((e2) => F.has(e2))) && r2(`VGPU-TEXTURE-USAGE-REQUIRED`, `Texture usage must be a nonempty list of known capabilities.`, `Pass explicit usage, such as ["texture_binding", "copy_dst"]. No capabilities are added automatically.`), (typeof e.format != `string` || !e.format) && r2(`VGPU-TEXTURE-FORMAT-REQUIRED`, `Texture format is required.`, `Pass an explicit GPUTextureFormat, such as "rgba16float".`);
    let a2 = e.mipLevelCount ?? 1, o2 = e.kind === `1d` ? 1 : Math.floor(Math.log2(Math.max(...e.size))) + 1;
    (!V(a2) || a2 > o2) && r2(`VGPU-TEXTURE-MIPS-INVALID`, `mipLevelCount must be between 1 and ${o2}.`, `Omit mipLevelCount for the base level, or request a valid allocation count. Contents are not generated automatically.`);
    let s3 = e.sampleCount ?? 1;
    s3 !== 1 && s3 !== 4 && r2(`VGPU-TEXTURE-SAMPLES-INVALID`, `sampleCount must be 1 or 4.`, `Omit sampleCount for single-sample storage.`), s3 === 4 && (e.kind !== `2d` || a2 !== 1 || !e.usage.includes(`render_attachment`) || e.usage.includes(`storage_binding`)) && r2(`VGPU-TEXTURE-SAMPLES-INVALID`, `MSAA requires a 2d render attachment with one mip and no storage_binding.`, `Use a single-sample texture for storage, arrays or volumes; resolve MSAA into a separate texture.`), e.kind === `1d` && (e.usage.includes(`render_attachment`) || /^(depth|stencil|bc|etc2|eac|astc)/.test(e.format)) && r2(`VGPU-TEXTURE-KIND-INVALID`, `1d textures cannot be render attachments or use depth/stencil/compressed formats.`, `Use a 2d texture or a plain color format with non-attachment usage.`);
    let c2 = z(e), l2 = e.kind === `1d` ? `maxTextureDimension1D` : e.kind === `3d` ? `maxTextureDimension3D` : `maxTextureDimension2D`, u3 = t4.limits?.[l2];
    (u3 !== void 0 && e.size.some((e2) => e2 > u3) || e.kind === `2d-array` && t4.limits?.maxTextureArrayLayers !== void 0 && c2[2] > t4.limits.maxTextureArrayLayers) && r2(`VGPU-TEXTURE-LIMIT`, `Texture dimensions or layers exceed enabled device limits.`, `Reduce the allocation or request sufficient limits when initializing the device.`);
    let d2 = (e2) => t4.features?.has(e2) ?? false;
    e.usage.includes(`storage_binding`) && !I.has(e.format) && !(e.format === `bgra8unorm` && d2(`bgra8unorm-storage`)) && !(L.has(e.format) && d2(`texture-formats-tier1`)) && r2(`VGPU-TEXTURE-STORAGE-FORMAT`, `Format ${e.format} does not support storage_binding with the enabled features.`, `Choose a supported storage format, enable the required format feature, or explicitly omit storage_binding from usage.`), e.viewFormats !== void 0 && (!Array.isArray(e.viewFormats) || !Array.from(e.viewFormats).every((t5) => typeof t5 == `string` && t5.replace(/-srgb$/, ``) === e.format.replace(/-srgb$/, ``))) && r2(`VGPU-TEXTURE-VIEW-FORMAT`, `viewFormats must list formats compatible with the texture format.`, `Only the same format or its compatible linear/sRGB counterpart is allowed; native device restrictions also apply.`);
  }
  function V(e) {
    return typeof e == `number` && Number.isSafeInteger(e) && e > 0;
  }
  function W(e) {
    let [t4, n2, r2] = z(e), i2 = { label: e.label, size: { width: t4, height: n2, depthOrArrayLayers: r2 }, dimension: e.kind === `2d-array` ? `2d` : e.kind, format: e.format, usage: u2(e.usage) };
    return e.mipLevelCount !== void 0 && (i2.mipLevelCount = e.mipLevelCount), e.sampleCount !== void 0 && (i2.sampleCount = e.sampleCount), e.viewFormats !== void 0 && (i2.viewFormats = [...e.viewFormats]), e.kind === `2d-array` && (i2.textureBindingViewDimension = `2d-array`), i2;
  }
  function K(e) {
    return !Number.isFinite(e.size) || e.size <= 0 ? q(`Buffer size must be greater than zero.`) : e.usage.length === 0 ? q(`Buffer usage must not be empty.`) : null;
  }
  function q(e) {
    return new n({ code: `VGPU-CORE-INVALID-USAGE`, message: e, where: `Device.createBuffer` });
  }
  function J(e) {
    return { label: e.label, size: e.size, usage: o(e.usage) };
  }
  function oe(e) {
    return e ? new n({ code: `VGPU-CORE-VALIDATION`, message: e.message, where: `GPUDevice.popErrorScope`, cause: e }) : null;
  }
  function se(e) {
    if (typeof e != `object` && typeof e != `function` || e === null) return false;
    let t4 = e;
    return Number.isSafeInteger(t4.size) && (t4.size ?? -1) >= 0 && Number.isSafeInteger(t4.usage) && (t4.usage ?? -1) >= 0 && typeof t4.destroy == `function`;
  }
  function le(e) {
    return ce.filter((t4) => (e & o([t4])) !== 0);
  }
  function ue(e, t4, n2 = `resource`) {
    return new Y({ code: `VGPU-R1-BINDING-DESTROYED`, message: `Binding '${t4.name}' (@group(${t4.group}) @binding(${t4.binding})) in '${e}' refers to destroyed resource '${n2}'.`, where: `${e}.${t4.name}`, fix: `Create a live resource and call set({ ${t4.name}: replacement }). Direct attachment references do not follow target.resize(); bind the Target itself to follow replacements. Re-record bundles that captured the old resource.`, detail: { binding: t4.binding, bindingName: t4.name, resourceName: n2 } });
  }
  function de(e, t4, n2, r2, i2, a2) {
    let o2 = t4 === `vertex` ? `Vertex` : `Fragment`, s3 = t4 === `vertex` ? `VERTEX` : `FRAGMENT`, c2 = `maxStorageBuffersIn${o2}Stage`;
    return new Y({ code: `VGPU-LIMIT-STORAGE-${s3}`, message: `${o2} entry '${n2}' in '${e}' uses ${r2} storage buffer(s), but device limit ${c2} is ${i2}.`, fix: t4 === `vertex` ? `Request init({ requiredLimits: { ${c2}: ${r2} } }) if the adapter supports it, or move vertex data to geometry(gpu, ...) vertex streams.` : `Request init({ requiredLimits: { ${c2}: ${r2} } }) if the adapter supports it, or reduce fragment storage buffers.`, where: `${e}.pipelineLayout`, detail: { stage: t4, entryPoint: n2, count: r2, limit: i2, bindings: a2.map(({ name: e2, group: t5, binding: n3 }) => ({ name: e2, group: t5, binding: n3 })) } });
  }
  function fe(e, t4, n2, r2, i2) {
    return new Y({ code: `VGPU-SET-TEXTURE-FILTERABILITY`, message: `${r2} (${n2}) cannot satisfy filtering texture '${t4.name}' @group(${t4.group}) @binding(${t4.binding}).`, fix: `Use a filterable format; request float32-filterable for rgba32float when supported; or use textureLoad without a sampler.`, where: `${e}.set`, detail: { format: n2, group: t4.group, binding: t4.binding, bindingName: t4.name, resourceName: r2, samplerName: i2?.name, samplerGroup: i2?.group, samplerBinding: i2?.binding } });
  }
  function pe(e, t4) {
    let n2 = ht(e, t4);
    return new Y({ code: `VGPU-R1-BINDING-NEVER-SET`, message: `Unset \`${t4.name}\` @group(${t4.group}) @binding(${t4.binding}) in '${e}'. Fix: ${n2}; or ${e}.group(${t4.group}, bindGroup).`, where: `${e}.draw` });
  }
  function me(e, t4) {
    return new Y({ code: `VGPU-R1-OWNERSHIP-FLIP`, message: `\`${e}\` is ${t4 === `lib` ? `lib-owned by its first JS set()` : `user-owned by its first resource set()`}; ownership cannot change. ${t4 === `lib` ? `Fix: pass a resource from the start: wave.set({ ${e}: new Uniform(gpu.device, { size: 4 }) }).` : `Fix: pass JS values from the first set(): wave.set({ ${e}: jsValue }).`}`, where: `set` });
  }
  function he(e, t4) {
    return new Y({ code: `VGPU-R4-GROUP-CLAIMED`, message: `group ${t4} of '${e}' is claimed; set() cannot update it.`, fix: `Call set() first, or build from ${e}.layout(${t4}); pass dynamic offsets to p.draw().`, where: `${e}.set` });
  }
  function ge(e, t4, n2, r2) {
    return new Y({ code: `VGPU-R4-GROUP-INCOMPATIBLE`, message: `claimed group ${t4} in '${e}' is incompatible: ${n2}.`, fix: `Build from ${e}.layout(${t4}, { dynamicOffsets? }) then call ${e}.group(${t4}, bindGroup).`, where: `${e}.group`, cause: r2 });
  }
  function _e(e, t4, n2) {
    return new Y({ code: `VGPU-R4-GROUP-VALIDATION`, message: `WebGPU rejected claimed group ${t4} in '${e}'.`, fix: `Build from ${e}.layout(${t4}); pass offsets via p.draw(draw, { offsets: { ${t4}: [...] } }).`, where: `${e}.draw`, cause: n2, detail: { drawLabel: e, group: t4 } });
  }
  function ve(e, t4) {
    return new Y({ code: `VGPU-BLEND-INVALID`, message: `Invalid blend '${String(t4)}' in '${e}'.`, fix: `Use "alpha", "additive", "premultiplied", or { color, alpha? } components.`, where: `draw` });
  }
  function ye(e, t4) {
    return new Y({ code: `VGPU-BLEND-CONSTANT-INVALID`, message: `Invalid blendConstant in '${e}': ${t4}`, fix: `Use [r, g, b, a] finite numbers with a blend whose color or alpha uses "constant"/"one-minus-constant"; omit it to keep the pass default (0, 0, 0, 0).`, where: `draw` });
  }
  function be(e, t4) {
    return new Y({ code: `VGPU-WRITEMASK-INVALID`, message: `Invalid writeMask ${t4} in '${e}'.`, fix: `Use an array of r/g/b/a; omit it for all channels.`, where: `draw` });
  }
  function xe(e, t4, n2 = `draw`) {
    return new Y({ code: `VGPU-COLORS-INVALID`, message: `Invalid colors in '${e}': ${t4}`, fix: `Use one { blend?, writeMask? } or null entry per color attachment of the target, aligned by index; omit colors to apply the top-level blend/writeMask to every attachment.`, where: n2 });
  }
  function Se(e, t4) {
    return new Y({ code: `VGPU-CULL-INVALID`, message: `Invalid cull '${String(t4)}' in '${e}'.`, fix: `Use "none", "front", or "back"; omit it for no culling.`, where: `draw` });
  }
  function Ce(e, t4) {
    return new Y({ code: `VGPU-FRONTFACE-INVALID`, message: `Invalid frontFace '${String(t4)}' in '${e}'.`, fix: `Use "ccw" or "cw"; omit it for counter-clockwise.`, where: `draw` });
  }
  function we(e, t4) {
    return new Y({ code: `VGPU-UNCLIPPED-DEPTH-INVALID`, message: `Invalid unclippedDepth in '${e}': ${t4}`, fix: `Use a boolean. unclippedDepth: true needs the "depth-clip-control" device feature \u2014 request it with init({ requiredFeatures: ["depth-clip-control"] }) on an adapter that supports it. Omit the option to keep depth clipping.`, where: `draw` });
  }
  function Te(e, t4) {
    return new Y({ code: `VGPU-DEPTH-INVALID`, message: `Invalid depth in '${e}': ${t4}`, fix: `Use false or { write?, compare?, bias?, biasSlopeScale?, biasClamp? }; omit it for { write: true, compare: "less-equal" }.`, where: `draw` });
  }
  function Ee(e, t4, n2 = `draw`) {
    return new Y({ code: `VGPU-STENCIL-INVALID`, message: `Invalid stencil in '${e}': ${t4}`, fix: `Use { front?, back?, readMask?, writeMask?, ref? } with GPUCompareFunction/GPUStencilOperation faces and u32 masks, against a target whose depth format has a stencil aspect (depth: "depth24plus-stencil8"); omit it for WebGPU's pass-through defaults.`, where: n2 });
  }
  function De(e, t4, n2 = `draw`) {
    return new Y({ code: `VGPU-MULTISAMPLE-INVALID`, message: `Invalid multisample in '${e}': ${t4}`, fix: `Use { alphaToCoverage?, mask? }: alphaToCoverage needs a target created with msaa: true, and mask must be an integer in [0, 0xFFFFFFFF] (bits above the target's sampleCount are ignored). Omit multisample for full-coverage defaults.`, where: n2 });
  }
  function Oe(e, t4, n2 = `draw`) {
    return new Y({ code: `VGPU-CONSTANTS-INVALID`, message: `Invalid constants in '${e}': ${t4}`, fix: "Key WGSL `override` constants by name, or by the decimal string of N when the declaration has @id(N); values are finite numbers or booleans, converted to the override's WGSL type (bool/i32/u32/f32/f16). Every override without a default value must be provided. Omit constants to keep the WGSL defaults.", where: n2 });
  }
  function ke(e, t4, n2 = `draw`) {
    return new Y({ code: `VGPU-ENTRY-INVALID`, message: `Invalid entry in '${e}': ${t4}`, fix: `Name an entry point declared in the shader with the matching stage \u2014 { vertex?, fragment? } strings for draw, one @compute name string for compute. Omit entry (or a field) to use the first entry point of that stage.`, where: n2 });
  }
  function Ae(e, t4, n2) {
    return new Y({ code: `VGPU-INDIRECT-INVALID`, message: `Invalid indirect in '${e}': ${t4}`, fix: `Pass a storage buffer created with storage(gpu, bytes, { indirect: true }) \u2014 bare, or as { buffer, offset? } with a 4-aligned byte offset \u2014 sized so the GPU-read arguments fit: 16 bytes for drawIndirect, 20 for drawIndexedIndirect, 12 for dispatchWorkgroupsIndirect. Omit indirect to use CPU-side counts.`, where: n2 });
  }
  function je() {
    return new Y({ code: `VGPU-PASS-PRESERVE-MSAA`, message: `clear:false cannot preserve MSAA; use a non-MSAA target.`, fix: `Use non-MSAA for accumulation.`, where: `Frame.pass` });
  }
  function Me(e, t4 = `expected a number in [0, 1].`, n2 = `Use 1 (default), or 0 with depth: { compare: "greater" } for reversed-Z.`) {
    return new Y({ code: `VGPU-PASS-CLEARDEPTH-INVALID`, message: `clearDepth received ${String(e)}; ${t4}`, fix: n2, where: `Frame.pass` });
  }
  function Ne(e) {
    return new Y({ code: `VGPU-PASS-VIEWPORT-INVALID`, message: `Invalid viewport: ${e}`, fix: `Use { x?, y?, width, height, minDepth?, maxDepth? } finite numbers within device limits; omit it for the full target.`, where: `Frame.pass` });
  }
  function Pe(e) {
    return new Y({ code: `VGPU-PASS-SCISSOR-INVALID`, message: `Invalid scissor: ${e}`, fix: `Use [x, y, width, height] non-negative integers with x + width and y + height within the target's current pixel size; omit it for the full target.`, where: `Frame.pass` });
  }
  function Fe() {
    return new Y({ code: `VGPU-PASS-PRESERVE-CLEARDEPTH`, message: `clear:false preserves depth; clearDepth cannot apply.`, fix: `Remove clearDepth, or let the pass clear.`, where: `Frame.pass` });
  }
  function Ie(e) {
    return new Y({ code: `VGPU-PASS-CLEARSTENCIL-INVALID`, message: `clearStencil ${e}`, fix: `Use an integer in [0, 0xFFFFFFFF] on a target whose depth format has a stencil aspect, e.g. depth: "depth24plus-stencil8"; the value is masked to the stencil aspect's bit width.`, where: `Frame.pass` });
  }
  function Le() {
    return new Y({ code: `VGPU-PASS-PRESERVE-CLEARSTENCIL`, message: `clear:false preserves stencil; clearStencil cannot apply.`, fix: `Remove clearStencil, or let the pass clear.`, where: `Frame.pass` });
  }
  function Re(e, t4, n2 = `Frame.pass`) {
    return new Y({ code: `VGPU-PASS-DEPTH-READONLY`, message: `depthReadOnly ${e}`, fix: t4, where: n2 });
  }
  function ze() {
    return new Y({ code: `VGPU-PASS-DEPTH-READONLY-MSAA`, message: `depthReadOnly cannot read an MSAA target's depth: multisampled depth is stored with storeOp "discard", so a read-only pass tests against discarded contents.`, fix: `Use a non-MSAA target for read-only depth, or drop depthReadOnly and let the pass own its depth.`, where: `Frame.pass` });
  }
  function Be(e, t4, n2 = `timer`) {
    return new Y({ code: `VGPU-TIMER-INVALID`, message: `Invalid timer use: ${e}`, fix: t4, where: n2 });
  }
  function Ve(e, t4, n2 = `visibility`) {
    return new Y({ code: `VGPU-VIS-INVALID`, message: `Invalid visibility use: ${e}`, fix: t4, where: n2 });
  }
  function He() {
    return new Y({ code: `VGPU-QUERY-NO-VISIBILITY`, message: `occlusion() needs the pass to be opened with a visibility instance; the render pass has no occlusionQuerySet to write into.`, fix: `Open the pass with f.pass({ target, visibility: vis }, ...) using the visibility(gpu) instance that created the query handle.`, where: `FramePass.occlusion` });
  }
  function Ue() {
    return new Y({ code: `VGPU-QUERY-NESTED`, message: `occlusion() cannot nest inside an active occlusion() body; WebGPU allows one active occlusion query per pass at a time.`, fix: `Encode each occlusion scope sequentially: p.occlusion(a, ...); p.occlusion(b, ...).`, where: `FramePass.occlusion` });
  }
  function We(e = `Frame.pass`) {
    return new Y({ code: `VGPU-TARGET-REQUIRED`, message: `Target required. Fix: pass surface(gpu, canvas) or target(gpu, { size }) as { target }.`, where: e });
  }
  function Ge(e, t4, n2, r2) {
    return new Y({ code: e, message: `${e}: ${n2}`, fix: r2, where: t4 });
  }
  function Ke(e, t4) {
    return Ge(`VGPU-MESH-RANGE-INVALID`, e, t4, `Use index ranges for indexed geometries, vertex ranges otherwise, within geometry counts.`);
  }
  function qe(e) {
    return new Y({ code: `VGPU-PIPELINE-LAYOUT-GAP`, message: `Pipeline bind group ${e} is missing.`, fix: `Use consecutive @group() indices starting at 0.`, where: `pipeline layout` });
  }
  function Je(e, t4, n2) {
    return new Y({ code: `VGPU-COMPILE-FAILED`, message: `WebGPU pipeline compilation failed.`, fix: `Check WGSL, vertex layouts, and target signature.`, where: e, cause: t4, detail: n2 ? { signature: n2 } : void 0 });
  }
  function Ye(e) {
    return new Y({ code: `VGPU-COMPILE-DISPOSED`, message: `GPU disposed during pipeline compilation.`, where: e });
  }
  function Xe(e, t4) {
    return new Y({ code: `VGPU-COMPILE-SIGNATURE-INVALID`, message: `Invalid TargetSignature: ${t4}`, fix: `Pass { colors, depth?, sampleCount?:1|4 } or a Target.`, where: e });
  }
  function Ze(e) {
    return new Y({ code: `VGPU-TARGET-DEPTH-STENCIL-ONLY`, message: `depth received '${e}'; stencil-only depth targets are not supported yet.`, fix: `Use a format with a depth aspect such as "depth24plus" or "depth24plus-stencil8".`, where: `target` });
  }
  function Qe() {
    return new Y({ code: `VGPU-TARGET-SIZE-REQUIRED`, message: `Target size required. Fix: target(gpu, { size: [w,h] }); update surface-derived targets in onResize.`, where: `target` });
  }
  function $e(e) {
    return new Y({ code: `VGPU-SURFACE-NOT-IN-FRAME`, message: `Surface targets are only available inside frame(gpu).`, fix: `surface passes must run inside frame(gpu, ...); precompile against an offscreen target(gpu, ...) instead`, where: e });
  }
  function et() {
    return new Y({ code: `VGPU-SURFACE-CONTEXT`, message: `Canvas WebGPU context failed. Fix: check navigator.gpu and remove any existing 2d/webgl context.`, where: `surface` });
  }
  function tt(e) {
    return new Y({ code: `VGPU-SURFACE-DUPLICATE`, message: `Canvas already has surface${e ? ` '${e}'` : ``}. Fix: reuse or dispose it.`, where: `surface` });
  }
  function nt(e) {
    return new Y({ code: `VGPU-SURFACE-DISPOSED`, message: `Surface '${e ?? `surface`}' is disposed. Fix: call surface(gpu, canvas).`, where: `surface` });
  }
  function rt() {
    return new Y({ code: `VGPU-SURFACE-AUTORESIZE-UNSUPPORTED`, message: `autoResize needs clientWidth. Fix: call surface.resize([w,h]) for OffscreenCanvas; onResize still fires.`, where: `surface` });
  }
  function it(e) {
    return new Y({ code: `VGPU-SURFACE-RESIZE-REENTRANT`, message: `Cannot resize this surface${e ? ` '${e}'` : ``} in onResize. Fix: resize derived targets only.`, where: `surface.resize` });
  }
  function at(e) {
    return new Y({ code: `VGPU-CLEAR-COLOR-INVALID`, message: `Invalid ${e}: expected four finite numbers.`, fix: `Assign [r, g, b, a] or a GPUColor object ({ r, g, b, a }).`, where: e });
  }
  function ot(e) {
    return new Y({ code: `VGPU-CLOCK-DELTA-INVALID`, message: `clock.advance() received ${String(e)}; expected a finite, non-negative number of seconds.`, fix: `Pass the elapsed seconds, e.g. clock(gpu).advance(1 / 60); use frame(gpu) alone to advance with wall-clock time.`, where: `clock.advance` });
  }
  function st() {
    return new Y({ code: `VGPU-FRAME-REENTRANT`, message: `Nested frame(gpu) is invalid. Fix: queue work for the next frame.`, where: `frame` });
  }
  function ct(e) {
    return new Y({ code: `VGPU-FRAME-CANCELED`, message: `the frame was canceled; its command encoder was dropped and nothing more can be encoded or submitted on it.`, fix: `Open a new frame(gpu) for further work; cancel() is the last operation on a frame.`, where: e });
  }
  function lt(e) {
    return new Y({ code: `VGPU-FRAME-PASS-ACTIVE`, message: `the frame cannot be canceled while a pass callback is active.`, fix: `Return from the frame.pass(...) callback first, then call frame.cancel(); this keeps pass descriptor resources alive until the pass is closed.`, where: e });
  }
  function ut(e) {
    return new Y({ code: `VGPU-FRAME-SUBMITTED`, message: `the frame was already submitted; submitted GPU work cannot be canceled.`, fix: `Call cancel() only on a frame you decided not to submit; the frame you did submit needs no cleanup.`, where: e });
  }
  function dt(e, t4, n2) {
    return new Y({ code: `VGPU-R1-BINDING-INCOMPATIBLE-RESOURCE`, message: `binding \`${e.name}\` @group(${e.group}) @binding(${e.binding}) needs ${t4}.`, fix: n2, where: `set` });
  }
  function X(e, t4, n2) {
    return new Y({ code: `VGPU-RING1-UNSUPPORTED`, message: t4, fix: n2, where: e });
  }
  function ft(e, t4) {
    return new Y({ code: `VGPU-SET-VALUE-INVALID`, message: `Invalid WGSL value at '${e.path}': ${t4}.`, fix: `Pass the exact reflected structure, vector, matrix, and array shapes; use integral in-range values for i32/u32.`, where: `set`, detail: e });
  }
  function pt(e) {
    return mt(e) && e.version !== 1 ? new Y({ code: `VGPU-SHADER-SOURCE-INVALID`, message: `VGPU-SHADER-SOURCE-INVALID: unsupported ShaderSource v${String(e.version)}; expected v1. Fix: update vgpu or regenerate it.`, where: `shader source` }) : new Y({ code: `VGPU-SHADER-SOURCE-INVALID`, message: `VGPU-SHADER-SOURCE-INVALID: expected WGSL or { version, wgsl }, got ${Z(e)}. Fix: configure @vgpu/wgsl loader-vite or loader-webpack.`, where: `shader source` });
  }
  function mt(e) {
    return typeof e == `object` && !!e && `version` in e;
  }
  function Z(e) {
    if (typeof e != `object` || !e) return typeof e;
    try {
      let t4 = JSON.stringify(e);
      return t4.length > 80 ? `${t4.slice(0, 77)}...` : t4;
    } catch {
      return `object`;
    }
  }
  function ht(e, t4) {
    switch (t4.kind) {
      case `sampler`:
        return `${e}.set({${t4.name}:sampler(gpu)})`;
      case `texture`:
        return `${e}.set({${t4.name}:scene.color})`;
      case `buffer`:
        return t4.addressSpace === `uniform` ? `${e}.set({${t4.name}:{ /* values */ }})` : `${e}.set({${t4.name}:buffer})`;
      default:
        return `${e}.set({${t4.name}:resource})`;
    }
  }
  function gt(e) {
    return { name: e };
  }
  function _t(e) {
    let t4 = $.get(e);
    if (!t4) throw new Y({ code: `VGPU-GPU-FOREIGN`, message: `This object was not created by init(); it has no vgpu kernel.`, fix: `Pass the gpu returned by init() from vgpu, vgpu/node or vgpu/mock.`, where: `gpu` });
    return t4;
  }
  function yt(e) {
    let t4 = new vt(e), n2 = { device: e, gpu: e.gpu, get disposed() {
      return t4.disposed;
    }, onError: (e2) => t4.addErrorListener(e2), settled: () => t4.settled(), dispose: () => {
      t4.dispose();
    } };
    return $.set(n2, t4), n2;
  }
  async function bt(e, t4 = {}, n2) {
    return yt(await xt(e, t4, n2));
  }
  async function xt(e, t4, n2) {
    if (t4.adapter || n2) return (t4.adapter ?? n2()).requestDevice(t4);
    if (e === `browser`) return St(t4);
    throw X(`init`, `init(${e}) requires adapterFactory.`);
  }
  async function St(e) {
    let t4 = await globalThis.navigator.gpu?.requestAdapter({ powerPreference: e.powerPreference });
    if (!t4) throw X(`init`, `navigator.gpu.requestAdapter() returned null.`);
    return i(t4.features, e.requiredFeatures), new G(await t4.requestDevice({ requiredFeatures: e.requiredFeatures, requiredLimits: e.requiredLimits }), t4.info ?? null);
  }
  function wt(e) {
    return bt(`browser`, e);
  }
  var t3, n, a, l, m, g, ___instances, e_fn, _a, _, T, E, k, N, P, F, I, L, H, _a2, U, _G_instances, e_fn2, _a3, G, ce, Y, Q, $, _e2, _t2, _n, _r, _i, _a4, _a5, vt, Ct;
  var init_dist_CW40cPHt = __esm({
    "../../../../private/tmp/agent-wave-build/dist-CW40cPHt.js"() {
      init_rolldown_runtime_C6BX6AHi();
      (function() {
        try {
          var e = typeof window < `u` ? window : typeof global < `u` ? global : typeof globalThis < `u` ? globalThis : typeof self < `u` ? self : {}, t4 = new e.Error().stack;
          t4 && (e._sentryDebugIds = e._sentryDebugIds || {}, e._sentryDebugIds[t4] = `0d5725c3-b6ff-49c3-9de0-4f273b7428d3`, e._sentryDebugIdIdentifier = `sentry-dbid-0d5725c3-b6ff-49c3-9de0-4f273b7428d3`);
        } catch {
        }
      })();
      t3 = class extends Error {
        constructor(e) {
          super(e.message, { cause: e.cause });
          __publicField(this, "code");
          __publicField(this, "severity");
          __publicField(this, "fix");
          __publicField(this, "where");
          __publicField(this, "cause");
          __publicField(this, "detail");
          this.name = `VGPUError`, this.code = e.code, this.severity = e.severity ?? `error`, this.fix = e.fix, this.where = e.where, this.cause = e.cause, this.detail = e.detail;
        }
      };
      n = class extends t3 {
        constructor(e) {
          super({ ...e, severity: `error` }), this.name = `ValidationError`;
        }
      };
      a = { map_read: 1, map_write: 2, copy_src: 4, copy_dst: 8, index: 16, vertex: 32, uniform: 64, storage: 128, indirect: 256, query_resolve: 512 };
      l = { copy_src: 1, copy_dst: 2, texture_binding: 4, storage_binding: 8, render_attachment: 16 };
      m = 1;
      g = class {
        constructor() {
          __publicField(this, "callbacks", /* @__PURE__ */ new Set());
          __publicField(this, "destroyed", false);
        }
        onDestroy(e, t4) {
          return this.destroyed ? (t4(e), () => void 0) : (this.callbacks.add(t4), () => {
            this.callbacks.delete(t4);
          });
        }
        emit(e) {
          if (this.destroyed) return false;
          this.destroyed = true;
          let t4 = [...this.callbacks];
          this.callbacks.clear();
          let n2 = [];
          for (let r2 of t4) try {
            r2(e);
          } catch (e2) {
            n2.push(e2);
          }
          if (n2.length) throw n2[0];
          return true;
        }
      };
      _ = (_a = class {
        constructor(e, t4, n2, r2 = `owned`) {
          __privateAdd(this, ___instances);
          __publicField(this, "device");
          __publicField(this, "gpu");
          __publicField(this, "options");
          __publicField(this, "ownership");
          __publicField(this, "destroySignal", new g());
          __publicField(this, "identity", h(`buffer`));
          __publicField(this, "destroyed", false);
          this.device = e, this.gpu = t4, this.options = n2, this.ownership = r2, Object.defineProperty(this, "assertUsable", { value: (e2) => __privateMethod(this, ___instances, e_fn).call(this, e2) });
        }
        get resourceIdentity() {
          return this.identity;
        }
        onDestroy(e) {
          return this.destroySignal.onDestroy(this, e);
        }
        write(e, t4 = 0) {
          __privateMethod(this, ___instances, e_fn).call(this, `Buffer.write`), this.ownership === `external` && this.validateExternalOperation(`write`, t4, e.byteLength, `copy_dst`);
          try {
            this.device.queue.writeBuffer(this.gpu, t4, e);
          } catch (e2) {
            throw this.ownership === `external` ? v(`Buffer.write`, `The external GPUBuffer rejected the write operation.`, e2) : e2;
          }
        }
        async read(e, t4 = 0) {
          __privateMethod(this, ___instances, e_fn).call(this, `Buffer.read`), this.ownership === `external` && this.validateExternalOperation(`read`, t4, e, `copy_src`);
          try {
            let n2 = await this.device.readback.read(this.gpu, e, t4);
            return __privateMethod(this, ___instances, e_fn).call(this, `Buffer.read`), n2;
          } catch (e2) {
            throw e2 instanceof n || this.ownership !== `external` ? e2 : v(`Buffer.read`, `The external GPUBuffer rejected the read operation.`, e2);
          }
        }
        destroy() {
          this.destroyed || (this.destroyed = true, this.destroySignal.emit(this), this.ownership === `owned` && !f(this.gpu) && this.gpu.destroy());
        }
        dispose() {
          this.destroy();
        }
        validateExternalOperation(e, t4, n2, r2) {
          if (!(Number.isSafeInteger(t4) && t4 >= 0 && t4 % 4 == 0 && Number.isSafeInteger(n2) && n2 >= 0 && n2 % 4 == 0 && t4 <= this.options.size && n2 <= this.options.size - t4)) throw v(`Buffer.${e}`, `External buffer offsets and lengths must be non-negative, 4-byte aligned, and within the buffer size.`);
          if ((this.gpu.usage & o([r2])) === 0) throw v(`Buffer.${e}`, `External buffer is missing ${r2.toUpperCase()} usage.`);
        }
      }, ___instances = new WeakSet(), e_fn = function(e = `Buffer`) {
        if (this.destroyed) throw new n({ code: `VGPU-BUFFER-DISPOSED`, message: `Buffer is destroyed.`, where: e, fix: `Wrap or create a live GPUBuffer before using it.` });
        this.device.assertUsable(e);
      }, _a);
      T = o([`copy_dst`, `map_read`]);
      E = class {
        constructor(e) {
          __publicField(this, "device");
          this.device = e;
        }
        async read(e, t4, n2) {
          if (f(e)) return e.__vgpuMockBytes.slice(n2, n2 + t4).buffer;
          let r2 = this.device.createBuffer({ size: t4, usage: T });
          try {
            let i2 = this.device.createCommandEncoder();
            i2.copyBufferToBuffer(e, n2, r2, 0, t4), this.device.queue.submit([i2.finish()]), await r2.mapAsync(c());
            let a2 = r2.getMappedRange().slice(0);
            return D(r2), a2;
          } finally {
            O(r2);
          }
        }
        async readTexture(e, t4) {
          let n2 = A(e.format, `Readback.readTexture`), r2 = C(e, t4, `Readback.readTexture`), [i2, a2, o2] = r2.size, s3 = n2.bytesPerPixel, l2 = ne(i2 * s3, 256), u3 = l2 * a2 * o2;
          if (w(u3, this.device.limits?.maxBufferSize, `Readback.readTexture`), p(e)) {
            let t5 = e.__vgpuMockMips?.[r2.mipLevel] ?? (r2.mipLevel === 0 ? e.__vgpuMockBytes : void 0);
            if (!t5) throw Error(`Mock texture has no storage for the selected mip.`);
            return ae(t5, r2, n2);
          }
          let d2 = this.device.createBuffer({ size: u3, usage: T }), f2;
          try {
            let t5 = this.device.createCommandEncoder();
            t5.copyTextureToBuffer({ texture: e, mipLevel: r2.mipLevel, origin: r2.origin }, { buffer: d2, bytesPerRow: l2, rowsPerImage: a2 }, { width: i2, height: a2, depthOrArrayLayers: o2 }), this.device.queue.submit([t5.finish()]), await d2.mapAsync(c());
            let n3 = new Uint8Array(d2.getMappedRange());
            f2 = new Uint8Array(i2 * a2 * o2 * s3);
            for (let e2 = 0; e2 < a2 * o2; e2++) {
              let t6 = e2 * l2, r3 = e2 * i2 * s3;
              f2.set(n3.subarray(t6, t6 + i2 * s3), r3);
            }
            D(d2);
          } finally {
            O(d2);
          }
          return n2.swizzle === `bgra-to-rgba` && j(f2), f2;
        }
        destroy() {
        }
      };
      k = { r8unorm: { bytesPerPixel: 1, components: 1, componentType: `unorm8` }, rg8unorm: { bytesPerPixel: 2, components: 2, componentType: `unorm8` }, rgba8unorm: { bytesPerPixel: 4, components: 4, componentType: `unorm8` }, "rgba8unorm-srgb": { bytesPerPixel: 4, components: 4, componentType: `unorm8` }, bgra8unorm: { bytesPerPixel: 4, components: 4, componentType: `unorm8`, swizzle: `bgra-to-rgba` }, "bgra8unorm-srgb": { bytesPerPixel: 4, components: 4, componentType: `unorm8`, swizzle: `bgra-to-rgba` }, r16float: { bytesPerPixel: 2, components: 1, componentType: `float16` }, rg16float: { bytesPerPixel: 4, components: 2, componentType: `float16` }, rgba16float: { bytesPerPixel: 8, components: 4, componentType: `float16` }, r32float: { bytesPerPixel: 4, components: 1, componentType: `float32` }, rg32float: { bytesPerPixel: 8, components: 2, componentType: `float32` }, rgba32float: { bytesPerPixel: 16, components: 4, componentType: `float32` } };
      N = class {
        constructor(e, t4 = () => void 0) {
          __publicField(this, "gpu");
          __publicField(this, "guard");
          this.gpu = e, this.guard = t4;
        }
        writeBuffer(e, t4, n2) {
          this.guard(`Queue.writeBuffer`), this.gpu.writeBuffer(e, t4, n2);
        }
        async flush() {
          this.guard(`Queue.flush`), await this.gpu.onSubmittedWorkDone?.(), this.guard(`Queue.flush`);
        }
      };
      P = class {
        constructor(e, t4) {
          __publicField(this, "gpu");
          __publicField(this, "resolved");
          this.gpu = e, this.resolved = t4;
        }
        dispose() {
        }
        get kind() {
          return this.resolved.kind;
        }
        get source() {
          return this.resolved.source;
        }
        get code() {
          return this.resolved.wgsl;
        }
        get entryPoints() {
          return this.resolved.entryPoints;
        }
        get stats() {
          return this.resolved.stats;
        }
      };
      F = /* @__PURE__ */ new Set([`copy_src`, `copy_dst`, `texture_binding`, `storage_binding`, `render_attachment`]);
      I = /* @__PURE__ */ new Set([`rgba8unorm`, `rgba8snorm`, `rgba8uint`, `rgba8sint`, `rgba16float`, `rgba16uint`, `rgba16sint`, `rgba32float`, `rgba32uint`, `rgba32sint`, `r32float`, `r32uint`, `r32sint`, `rg32float`, `rg32uint`, `rg32sint`]);
      L = /* @__PURE__ */ new Set([`r8unorm`, `r8snorm`, `r8uint`, `r8sint`, `rg8unorm`, `rg8snorm`, `rg8uint`, `rg8sint`, `r16uint`, `r16sint`, `r16float`, `rg16uint`, `rg16sint`, `rg16float`, `rgb10a2uint`, `rgb10a2unorm`, `rg11b10ufloat`, `r16unorm`, `r16snorm`, `rg16unorm`, `rg16snorm`, `rgba16unorm`, `rgba16snorm`]);
      H = /* @__PURE__ */ Symbol.for(`vgpu/Texture`);
      U = (_a2 = H, class {
        constructor(e, t4, n2, r2 = `owned`) {
          __publicField(this, "device");
          __publicField(this, "ownership");
          __publicField(this, _a2, true);
          __publicField(this, "destroySignal", new g());
          __publicField(this, "identity", h(`texture`));
          __publicField(this, "currentGpu");
          __publicField(this, "currentOptions");
          __publicField(this, "defaultView", null);
          __publicField(this, "destroyed", false);
          this.device = e, this.ownership = r2, this.currentGpu = t4, this.currentOptions = R(n2);
        }
        get gpu() {
          return this.currentGpu;
        }
        get options() {
          return this.currentOptions;
        }
        get size() {
          return this.options.size;
        }
        get format() {
          return this.options.format;
        }
        get usage() {
          return this.options.usage;
        }
        get mipLevelCount() {
          return this.options.mipLevelCount ?? 1;
        }
        get sampleCount() {
          return this.options.sampleCount ?? 1;
        }
        get kind() {
          return this.options.kind;
        }
        get layers() {
          return this.options.kind === `2d-array` ? this.options.layers : 1;
        }
        get dimension() {
          return this.options.kind === `2d-array` ? `2d` : this.options.kind;
        }
        get viewFormats() {
          return this.options.viewFormats ?? [];
        }
        get label() {
          return this.options.label;
        }
        get resourceIdentity() {
          return this.identity;
        }
        onDestroy(e) {
          return this.destroySignal.onDestroy(this, e);
        }
        get view() {
          return this.assertAlive(), this.defaultView ?? (this.defaultView = this.createView()), this.defaultView;
        }
        createView(e) {
          return this.assertAlive(`Texture.createView`), this.gpu.createView(this.kind === `2d-array` ? { ...e, dimension: e?.dimension ?? `2d-array` } : e);
        }
        async read(e) {
          this.assertAlive(`Texture.read`);
          let t4 = await this.device.readback.readTexture(this.gpu, e);
          return this.assertAlive(`Texture.read`), t4;
        }
        async readFloats(e) {
          this.assertAlive(`Texture.readFloats`);
          let t4 = A(this.options.format, `Texture.readFloats`), { size: n2 } = C(this.gpu, e, `Texture.readFloats`);
          return w(n2[0] * n2[1] * n2[2] * t4.components * 4, this.device.gpu.limits?.maxBufferSize, `Texture.readFloats`), re(await this.read(e), this.options.format);
        }
        destroy() {
          if (!this.destroyed) {
            this.destroyed = true, this.defaultView = null;
            try {
              this.destroySignal.emit(this);
            } finally {
              this.ownership !== `external` && !p(this.gpu) && this.gpu.destroy();
            }
          }
        }
        dispose() {
          this.destroy();
        }
        assertAlive(e = `Texture`) {
          if (this.destroyed) throw new n({ code: `VGPU-CORE-TEXTURE-DESTROYED`, message: `Texture is destroyed`, where: e });
          this.device.assertUsable?.(e);
        }
      });
      G = (_a3 = class {
        constructor(e, t4 = null, n2 = `owned`, r2 = {}) {
          __privateAdd(this, _G_instances);
          __publicField(this, "gpu");
          __publicField(this, "adapterInfo");
          __publicField(this, "queue");
          __publicField(this, "readback");
          __publicField(this, "isCompatibilityMode");
          __publicField(this, "scopes", []);
          __publicField(this, "ownership");
          __publicField(this, "state", `alive`);
          __publicField(this, "lossInfo");
          __publicField(this, "observeLoss", true);
          this.gpu = e, this.adapterInfo = t4, Object.defineProperty(this, "assertUsable", { value: (e2) => __privateMethod(this, _G_instances, e_fn2).call(this, e2) }), this.ownership = typeof n2 == `string` ? n2 : `owned`;
          let i2 = typeof n2 == `string` ? r2 : n2;
          this.isCompatibilityMode = i2.isCompatibilityMode ?? false, this.queue = new N(e.queue, (e2) => __privateMethod(this, _G_instances, e_fn2).call(this, e2)), this.readback = new E(e);
          let a2 = e.lost;
          a2 && typeof a2.then == `function` && Promise.resolve(a2).then((e2) => {
            !this.observeLoss || this.state !== `alive` || (this.lossInfo = e2, this.state = `lost`);
          }, () => void 0);
        }
        get limits() {
          return __privateMethod(this, _G_instances, e_fn2).call(this, `Device.limits`), this.gpu.limits;
        }
        get features() {
          return __privateMethod(this, _G_instances, e_fn2).call(this, `Device.features`), this.gpu.features;
        }
        createShader(e) {
          __privateMethod(this, _G_instances, e_fn2).call(this, `Device.createShader`);
          let t4 = typeof e == `string` ? y(e) : e;
          return new P(this.gpu.createShaderModule({ code: t4.wgsl }), t4);
        }
        createTexture(e) {
          __privateMethod(this, _G_instances, e_fn2).call(this, `Device.createTexture`), B(e, this.gpu);
          let t4 = R(e);
          return new U(this, this.gpu.createTexture(W(t4)), t4);
        }
        createBuffer(e) {
          __privateMethod(this, _G_instances, e_fn2).call(this, `Device.createBuffer`);
          let t4 = K(e);
          t4 && this.captureError(t4);
          let n2 = t4 ? M(Math.max(4, e.size || 4)) : J(e);
          return new _(this, this.gpu.createBuffer(n2), e);
        }
        wrapBuffer(e) {
          if (__privateMethod(this, _G_instances, e_fn2).call(this, `Device.wrapBuffer`), !se(e)) throw new n({ code: `VGPU-EXTERNAL-BUFFER-INVALID`, message: `Device.wrapBuffer requires a GPUBuffer with finite size and usage properties.`, where: `Device.wrapBuffer`, fix: `Pass a live GPUBuffer created for this GPUDevice.` });
          let t4 = { size: e.size, usage: le(e.usage), ...e.label ? { label: e.label } : {} };
          return new _(this, e, t4, `external`);
        }
        pushErrorScope(e) {
          __privateMethod(this, _G_instances, e_fn2).call(this, `Device.pushErrorScope`), this.scopes.push([]), this.gpu.pushErrorScope?.(e);
        }
        async popErrorScope() {
          __privateMethod(this, _G_instances, e_fn2).call(this, `Device.popErrorScope`);
          let e = this.scopes.pop(), t4 = await this.gpu.popErrorScope?.();
          return __privateMethod(this, _G_instances, e_fn2).call(this, `Device.popErrorScope`), e?.[0] ?? oe(t4) ?? null;
        }
        destroy() {
          if (this.state === `disposed`) return;
          let e = this.state === `lost`;
          this.state = `disposed`, this.observeLoss = false, this.scopes.length = 0, this.readback.destroy(), this.ownership === `owned` && !e && this.gpu.destroy();
        }
        dispose() {
          this.destroy();
        }
        captureError(e) {
          let t4 = this.scopes.at(-1);
          if (t4) t4.push(e);
          else throw e;
        }
      }, _G_instances = new WeakSet(), e_fn2 = function(e) {
        if (this.state === `alive`) return;
        if (this.state === `disposed`) throw new n({ code: `VGPU-DEVICE-DISPOSED`, message: `The GPU device wrapper has been disposed.`, where: e, fix: `Create a new Gpu instance before performing more work.` });
        let t4 = this.lossInfo?.reason, r2 = this.lossInfo?.message;
        throw new n({ code: `VGPU-DEVICE-LOST`, message: `The GPU device was lost${t4 ? ` (${t4})` : ``}${r2 ? `: ${r2}` : `.`}`, where: e, cause: this.lossInfo });
      }, _a3);
      ce = [`map_read`, `map_write`, `copy_src`, `copy_dst`, `index`, `vertex`, `uniform`, `storage`, `indirect`, `query_resolve`];
      Y = class extends t3 {
      };
      Q = [`scheduler`, `resource`, `service`];
      $ = /* @__PURE__ */ new WeakMap();
      vt = (_a5 = class {
        constructor(e) {
          __publicField(this, "device");
          __privateAdd(this, _e2, /* @__PURE__ */ new Map());
          __privateAdd(this, _t2, new Map(Q.map((e) => [e, /* @__PURE__ */ new Set()])));
          __privateAdd(this, _n, /* @__PURE__ */ new Set());
          __privateAdd(this, _r, /* @__PURE__ */ new Set());
          __privateAdd(this, _i, /* @__PURE__ */ new Set());
          __privateAdd(this, _a4, false);
          this.device = e;
        }
        get disposed() {
          return __privateGet(this, _a4);
        }
        service(e, t4) {
          let n2 = __privateGet(this, _e2).get(e);
          if (n2 !== void 0) return n2;
          let r2 = t4(this);
          return __privateGet(this, _e2).set(e, r2), r2;
        }
        peekService(e) {
          return __privateGet(this, _e2).get(e);
        }
        own(e, t4) {
          let n2 = __privateGet(this, _t2).get(e);
          return n2.add(t4), () => {
            n2.delete(t4);
          };
        }
        addErrorListener(e) {
          return __privateGet(this, _n).add(e), () => {
            __privateGet(this, _n).delete(e);
          };
        }
        reportError(e) {
          if (__privateGet(this, _a4)) return Promise.resolve();
          let t4 = Promise.resolve().then(() => {
            let t5 = [...__privateGet(this, _n)];
            if (!t5.length) {
              console.error(e);
              return;
            }
            for (let n2 of t5) try {
              n2(e);
            } catch (e2) {
              console.error(e2);
            }
          });
          return this.trackDelivery(t4);
        }
        trackDelivery(e) {
          let t4 = Promise.resolve(e).then(() => void 0, (e2) => {
            console.error(e2);
          });
          return __privateGet(this, _r).add(t4), t4.finally(() => __privateGet(this, _r).delete(t4)), t4;
        }
        registerSettledSource(e) {
          return __privateGet(this, _i).add(e), () => {
            __privateGet(this, _i).delete(e);
          };
        }
        async settled() {
          let e = [...__privateGet(this, _r), ...[...__privateGet(this, _i)].flatMap((e2) => e2())];
          await Promise.allSettled(e);
        }
        dispose() {
          if (!__privateGet(this, _a4)) {
            __privateSet(this, _a4, true);
            for (let e of Q) {
              let t4 = __privateGet(this, _t2).get(e);
              for (let e2 of [...t4]) e2();
              t4.clear();
            }
            __privateGet(this, _e2).clear(), __privateGet(this, _i).clear(), __privateGet(this, _n).clear(), this.device.dispose();
          }
        }
      }, _e2 = new WeakMap(), _t2 = new WeakMap(), _n = new WeakMap(), _r = new WeakMap(), _i = new WeakMap(), _a4 = new WeakMap(), _a5);
      Ct = s({ init: () => wt });
    }
  });

  // ../../../../private/tmp/agent-wave-build/renderer-BGT6fWD2.js
  var renderer_BGT6fWD2_exports = {};
  __export(renderer_BGT6fWD2_exports, {
    createRenderer: () => Tc
  });

  // ../../../../private/tmp/agent-wave-build/preload-helper-DFqAasJE.js
  var t = (load) => load();

  // ../../../../private/tmp/agent-wave-build/renderer-BGT6fWD2.js
  init_dist_CW40cPHt();
  var __vite__mapDeps = (i2, m2 = __vite__mapDeps, d2 = m2.f || (m2.f = ["assets/dist-CW40cPHt.js", "assets/rolldown-runtime-C6BX6AHi.js"])) => i2.map((i3) => d2[i3]);
  (function() {
    try {
      var e = typeof window < `u` ? window : typeof global < `u` ? global : typeof globalThis < `u` ? globalThis : typeof self < `u` ? self : {}, t4 = new e.Error().stack;
      t4 && (e._sentryDebugIds = e._sentryDebugIds || {}, e._sentryDebugIds[t4] = `c80fcd57-de14-49ba-a866-ff4dab9bdf45`, e._sentryDebugIdIdentifier = `sentry-dbid-c80fcd57-de14-49ba-a866-ff4dab9bdf45`);
    } catch {
    }
  })();
  var Le2 = new Set(`alias.break.case.const.const_assert.continue.continuing.default.diagnostic.discard.else.enable.false.fn.for.if.let.loop.override.requires.return.struct.switch.true.var.while`.split(`.`));
  var Re2 = /* @__PURE__ */ new Set([`import`, `export`, `from`, `as`]);
  var ze2 = /* @__PURE__ */ new Set([...Le2, ...Re2]);
  var Be2 = /* @__PURE__ */ new WeakMap();
  var Ve2 = /* @__PURE__ */ new WeakMap();
  function He2(e, t4) {
    return Be2.set(e, Ge2(t4)), e;
  }
  function Ue2(e) {
    return Be2.get(e);
  }
  function We2(e) {
    return Ve2.get(e);
  }
  function Ge2(e) {
    return { entries: e.entries.map((e2) => ({ ...e2 })) };
  }
  function k2(e, t4) {
    e.assertUsable(t4);
  }
  function Ke2(e, t4) {
    e.assertUsable(t4);
  }
  var qe2 = /* @__PURE__ */ Symbol(`vgpu.bindingResource`);
  function Je2(e) {
    return typeof (typeof e == `object` && e ? e[qe2] : void 0) == `function` ? e : void 0;
  }
  var Ye2 = /* @__PURE__ */ Symbol(`vgpu.geometry.layoutResolver`);
  function A2(e, t4) {
    let n2 = _t(e);
    if (n2.disposed) throw Xe2(t4);
    return n2;
  }
  function Xe2(e) {
    return new Y({ code: `VGPU-GPU-DISPOSED`, message: `${e}() ran after gpu.dispose(); the device and everything it owned are gone.`, fix: `Create resources before disposing the gpu, or init() a new one.`, where: e });
  }
  var Ze2 = class extends Error {
    constructor(e, t4, n2 = 1, r2 = 1, i2 = `error`) {
      super(t4);
      __publicField(this, "code");
      __publicField(this, "line");
      __publicField(this, "column");
      __publicField(this, "severity");
      __publicField(this, "metadata");
      __publicField(this, "relatedDiagnostics");
      __publicField(this, "fix");
      __publicField(this, "where");
      __publicField(this, "cause");
      this.name = `VGPUError`, this.code = e, this.line = n2, this.column = r2, this.severity = i2;
    }
  };
  function Qe2(e, t4, n2 = {}) {
    let r2 = new Ze2(e, t4, n2.line ?? 1, n2.column ?? 1, n2.severity ?? `error`);
    return n2.fix !== void 0 && (r2.fix = n2.fix), n2.where !== void 0 && (r2.where = n2.where), n2.cause !== void 0 && (r2.cause = n2.cause), n2.metadata !== void 0 && (r2.metadata = n2.metadata), r2;
  }
  function j2(e, t4, n2 = 1, r2 = 1) {
    return new Ze2(e, t4, n2, r2);
  }
  var $e2 = /* @__PURE__ */ new Set([`fn`, `struct`, `const`, `alias`, `var`, `override`]);
  function et2(e) {
    let t4 = [], n2 = [], r2 = [], i2 = 0, a2 = false, o2 = 0;
    for (; i2 < e.length; ) {
      let s3 = e[i2];
      if (s3.text === `{`) {
        o2++, i2++;
        continue;
      }
      if (s3.text === `}`) {
        o2 = Math.max(0, o2 - 1), i2++;
        continue;
      }
      if (st2(s3)) {
        i2++;
        continue;
      }
      if (o2 > 0) {
        i2++;
        continue;
      }
      if (s3.text === `import`) {
        if (a2) throw j2(`VGPU-WGSL-IMP-ORDER`, `Imports must precede declarations`, s3.line, s3.column);
        let [n3, r3] = tt2(e, i2);
        t4.push(n3), i2 = r3;
        continue;
      }
      if (s3.text === `export` && e[i2 + 1]?.text === `{`) throw j2(`VGPU-WGSL-EXP-REEXPORT-CYCLE`, `Re-export cycles are not supported`, s3.line, s3.column);
      if (s3.text === `@` && e[i2 + 2]?.text === `export` && e[i2 + 3]?.text === `@`) throw j2(`VGPU-WGSL-EXP-NOTDECL`, `Repeated export attributes`, s3.line, s3.column);
      let c2 = s3.text === `export` || s3.text === `@` && e[i2 + 2]?.text === `export`, l2 = c2 ? nt2(e, s3.text === `export` ? i2 + 1 : i2 + 3) : i2, u3 = e[l2];
      if (u3 && $e2.has(u3.text)) {
        let t5 = it2(e, l2);
        n2.push({ name: t5, localName: t5, kind: u3.text }), c2 && r2.push({ name: t5, localName: t5, kind: u3.text }), a2 = true, i2 = l2 + 1;
        continue;
      }
      i2++;
    }
    return { imports: t4, exports: r2, locals: n2 };
  }
  function tt2(e, t4) {
    let n2 = t4 + 1, r2 = [];
    if (e[n2]?.text === `{`) {
      for (n2++; e[n2] && e[n2].text !== `}`; ) {
        if (st2(e[n2])) {
          n2++;
          continue;
        }
        let t5 = ot2(e[n2]), i3 = t5;
        n2++, e[n2]?.text === `as` && (i3 = ot2(e[n2 + 1]), n2 += 2), r2.push({ imported: t5, local: i3 }), e[n2]?.text === `,` && n2++;
      }
      n2++, at2(e[n2], `from`), n2++;
    } else if (e[n2]?.text === `*`) at2(e[n2 + 1], `as`), r2.push({ imported: `*`, local: ot2(e[n2 + 2]), namespace: true }), n2 += 3, at2(e[n2], `from`), n2++;
    else if (e[n2]?.kind === `string`) throw j2(`VGPU-WGSL-IMP-SIDEEFFECT`, `Side-effect imports are not supported`, e[n2].line, e[n2].column);
    else throw j2(`VGPU-WGSL-IMP-DEFAULT`, `Default imports are not supported`, e[n2]?.line, e[n2]?.column);
    let i2 = e[n2];
    if (i2?.kind !== `string`) throw j2(`VGPU-WGSL-RES-NOTFOUND`, `Import path must be a string`, i2?.line, i2?.column);
    let a2 = i2.text.slice(1, -1);
    return n2++, e[n2]?.text === `;` && n2++, [{ from: a2, bindings: r2, start: e[t4].start, end: e[n2 - 1].end }, n2];
  }
  function nt2(e, t4) {
    for (t4 = rt2(e, t4); e[t4]?.text === `@`; ) if (t4 = rt2(e, rt2(e, t4 + 1) + 1), e[t4]?.text === `(`) {
      let n2 = 0;
      do
        e[t4]?.text === `(` ? n2++ : e[t4]?.text === `)` && n2--, t4++;
      while (e[t4] && n2 > 0);
      t4 = rt2(e, t4);
    }
    return t4;
  }
  function rt2(e, t4) {
    for (; e[t4] && st2(e[t4]); ) t4++;
    return t4;
  }
  function it2(e, t4) {
    let n2 = t4 + 1;
    if (e[t4]?.text === `var` && e[n2]?.text === `<`) for (; e[n2] && e[n2].text !== `>`; ) n2++;
    for (; n2 < e.length; n2++) if (e[n2].kind === `ident`) return e[n2].text;
    throw j2(`VGPU-WGSL-EXP-NOTDECL`, `Exported declaration has no name`, e[t4]?.line, e[t4]?.column);
  }
  function at2(e, t4) {
    if (e?.text !== t4) throw j2(`VGPU-WGSL-IMP-DEFAULT`, `Expected ${t4}`, e?.line, e?.column);
  }
  function ot2(e) {
    if (e?.kind !== `ident`) throw j2(`VGPU-WGSL-IMP-DEFAULT`, `Expected identifier`, e?.line, e?.column);
    return e.text;
  }
  function st2(e) {
    return e.kind === `lineComment` || e.kind === `blockComment`;
  }
  function ct2(e, t4) {
    return t4 === `uniform` || t4 === `storage` ? `buffer` : e.kind === `sampler` ? `sampler` : e.kind === `texture` ? e.textureKind === `texture_external` ? `externalTexture` : `texture` : `unknown`;
  }
  function lt2(e, t4, n2, r2, i2) {
    if (e === `buffer`) return ut2(t4, n2, i2);
    if (r2.kind === `sampler`) return dt2(r2);
    if (r2.kind === `texture`) return r2.textureKind === `texture_external` ? { kind: `externalTexture`, externalTexture: {} } : r2.textureKind.startsWith(`texture_storage_`) ? ft2(r2) : pt2(r2);
  }
  function ut2(e, t4, n2) {
    return { kind: `buffer`, buffer: { type: e === `uniform` ? `uniform` : t4 === `read` ? `read-only-storage` : `storage`, hasDynamicOffset: false, minBindingSize: n2?.size } };
  }
  function dt2(e) {
    return { kind: `sampler`, sampler: { type: e.comparison ? `comparison` : `filtering` } };
  }
  function ft2(e) {
    return { kind: `storageTexture`, storageTexture: { access: gt2(e.access), format: e.texelFormat ?? `rgba8unorm`, viewDimension: ht2(e.dimension) } };
  }
  function pt2(e) {
    return { kind: `texture`, texture: { sampleType: mt2(e), viewDimension: ht2(e.dimension), multisampled: e.dimension === `multisampled_2d` || e.dimension === `depth_multisampled_2d` } };
  }
  function mt2(e) {
    if (e.textureKind.startsWith(`texture_depth_`)) return `depth`;
    let t4 = e.sampleType;
    return t4?.kind === `scalar` && t4.name === `i32` ? `sint` : t4?.kind === `scalar` && t4.name === `u32` ? `uint` : `unfilterable-float`;
  }
  function ht2(e) {
    switch (e) {
      case `1d`:
        return `1d`;
      case `2d_array`:
      case `depth_2d_array`:
        return `2d-array`;
      case `cube`:
      case `depth_cube`:
        return `cube`;
      case `cube_array`:
      case `depth_cube_array`:
        return `cube-array`;
      case `3d`:
        return `3d`;
      default:
        return `2d`;
    }
  }
  function gt2(e) {
    return e === `read` ? `read-only` : e === `read_write` ? `read-write` : `write-only`;
  }
  var _t3 = `VGPU-WGSL-IDENT-NONASCII`;
  var vt2 = `https://github.com/vercel-labs/vgpu/issues/294`;
  function yt2(e, t4) {
    let n2 = [], r2 = 0, i2 = 1, a2 = 1, o2 = (t5, r3, i3, a3, o3) => n2.push({ kind: t5, text: e.slice(r3, i3), start: r3, end: i3, line: a3, column: o3 }), s3 = () => {
      e[r2] === `
` ? (i2++, a2 = 1) : a2++, r2++;
    };
    for (; r2 < e.length; ) {
      let n3 = e[r2];
      if (/\s/.test(n3)) {
        s3();
        continue;
      }
      let c2 = r2, l2 = i2, u3 = a2;
      if (n3 === `/` && e[r2 + 1] === `/`) {
        for (; r2 < e.length && e[r2] !== `
`; ) s3();
        o2(`lineComment`, c2, r2, l2, u3);
        continue;
      }
      if (n3 === `/` && e[r2 + 1] === `*`) {
        let t5 = 0;
        for (; r2 < e.length; ) {
          if (e[r2] === `/` && e[r2 + 1] === `*`) {
            t5++, s3(), s3();
            continue;
          }
          if (e[r2] === `*` && e[r2 + 1] === `/`) {
            if (t5--, s3(), s3(), t5 === 0) {
              o2(`blockComment`, c2, r2, l2, u3);
              break;
            }
            continue;
          }
          s3();
        }
        if (t5 !== 0) throw j2(`VGPU-WGSL-LEX-UNTERM-COMMENT`, `Unterminated block comment`, l2, u3);
        continue;
      }
      if (n3 === `"` || n3 === `'`) {
        let t5 = n3;
        for (s3(); r2 < e.length && e[r2] !== t5; ) {
          if (e[r2] === `
`) throw j2(`VGPU-WGSL-LEX-UNTERM-STRING`, `Unterminated string`, l2, u3);
          e[r2] === `\\` && s3(), s3();
        }
        if (r2 >= e.length) throw j2(`VGPU-WGSL-LEX-UNTERM-STRING`, `Unterminated string`, l2, u3);
        s3(), o2(`string`, c2, r2, l2, u3);
        continue;
      }
      if (/[A-Za-z_]/.test(n3)) {
        for (; r2 < e.length && /[A-Za-z0-9_]/.test(e[r2]); ) s3();
        let t5 = e.slice(c2, r2);
        o2(ze2.has(t5) ? `keyword` : `ident`, c2, r2, l2, u3);
        continue;
      }
      if (/[0-9]/.test(n3) || n3 === `.` && /[0-9]/.test(e[r2 + 1] ?? ``)) {
        for (n3 === `.` && s3(); r2 < e.length; ) {
          let t5 = e[r2];
          if (/[A-Za-z0-9_.]/.test(t5)) {
            s3();
            continue;
          }
          if ((t5 === `+` || t5 === `-`) && St2(e[r2 - 1]) && /[0-9]/.test(e[r2 + 1] ?? ``)) {
            s3();
            continue;
          }
          break;
        }
        o2(`number`, c2, r2, l2, u3);
        continue;
      }
      if (n3.charCodeAt(0) > 127) throw bt2(e, r2, i2, a2, t4);
      s3(), o2(`punct`, c2, r2, l2, u3);
    }
    return n2;
  }
  function bt2(e, t4, n2, r2, i2) {
    let a2 = t4;
    for (; a2 > 0 && xt2(e[a2 - 1]); ) a2--;
    let o2 = t4 + 1;
    for (; o2 < e.length && xt2(e[o2]); ) o2++;
    let s3 = e.slice(a2, o2), c2 = r2 - (t4 - a2), l2 = Qe2(_t3, `Non-ASCII identifier '${s3}'${i2 === void 0 ? `` : ` in ${i2}`} at line ${n2} column ${c2}; vgpu's WGSL pipeline supports ASCII identifiers only`, { fix: `Rename '${s3}' using ASCII letters, digits and '_'. Unicode (XID) identifiers are tracked in ${vt2}`, line: n2, column: c2 });
    return l2.range = { file: i2, start: { line: n2, column: c2 } }, l2;
  }
  function xt2(e) {
    return e.charCodeAt(0) > 127 || /[A-Za-z0-9_]/.test(e);
  }
  function St2(e) {
    return e === `e` || e === `E` || e === `p` || e === `P`;
  }
  var M2 = (1n << 64n) - 1n;
  var N2 = 11400714785074694791n;
  var Ct2 = 14029467366897019727n;
  var wt2 = 1609587929392839161n;
  var Tt = 9650029242287828579n;
  var Et = 2870177450012600261n;
  function Dt(e, t4 = 0n) {
    let n2 = new TextEncoder().encode(e), r2 = 0, i2;
    if (n2.length >= 32) {
      let e2 = t4 + N2 + Ct2, a2 = t4 + Ct2, o2 = t4, s3 = t4 - N2, c2 = n2.length - 32;
      do
        e2 = Ot(e2, At(n2, r2)), r2 += 8, a2 = Ot(a2, At(n2, r2)), r2 += 8, o2 = Ot(o2, At(n2, r2)), r2 += 8, s3 = Ot(s3, At(n2, r2)), r2 += 8;
      while (r2 <= c2);
      i2 = P2(e2, 1n) + P2(a2, 7n) + P2(o2, 12n) + P2(s3, 18n), i2 = kt(i2, e2), i2 = kt(i2, a2), i2 = kt(i2, o2), i2 = kt(i2, s3);
    } else i2 = t4 + Et;
    for (i2 = i2 + BigInt(n2.length) & M2; r2 + 8 <= n2.length; ) i2 ^= Ot(0n, At(n2, r2)), i2 = P2(i2, 27n) * N2 + Tt & M2, r2 += 8;
    for (r2 + 4 <= n2.length && (i2 ^= jt(n2, r2) * N2 & M2, i2 = P2(i2, 23n) * Ct2 + wt2 & M2, r2 += 4); r2 < n2.length; ) i2 ^= BigInt(n2[r2]) * Et & M2, i2 = P2(i2, 11n) * N2 & M2, r2++;
    return i2 ^= i2 >> 33n, i2 = i2 * Ct2 & M2, i2 ^= i2 >> 29n, i2 = i2 * wt2 & M2, i2 ^= i2 >> 32n, i2.toString(16).padStart(16, `0`);
  }
  function Ot(e, t4) {
    return P2(e + t4 * Ct2 & M2, 31n) * N2 & M2;
  }
  function kt(e, t4) {
    return e ^= Ot(0n, t4), e * N2 + Tt & M2;
  }
  function P2(e, t4) {
    return (e << t4 | e >> 64n - t4) & M2;
  }
  function At(e, t4) {
    let n2 = 0n;
    for (let r2 = 7; r2 >= 0; r2--) n2 = (n2 << 8n) + BigInt(e[t4 + r2]);
    return n2;
  }
  function jt(e, t4) {
    return BigInt(e[t4]) | BigInt(e[t4 + 1]) << 8n | BigInt(e[t4 + 2]) << 16n | BigInt(e[t4 + 3]) << 24n;
  }
  function Mt(e) {
    return Dt(e);
  }
  function Nt(e) {
    return Mt(e).slice(0, 8);
  }
  function Pt(e, t4) {
    return `_vgsl_${Nt(e)}__${t4}`;
  }
  function F2(e, t4) {
    let n2 = e.find((e2) => e2.name === t4);
    if (!n2) return;
    let r2 = n2.args.map((e2) => e2.text).join(``), i2 = Number(r2.replace(/[ui]$/, ``));
    return Number.isFinite(i2) ? i2 : void 0;
  }
  function Ft(e) {
    let t4 = [[]], n2 = 0, r2 = 0;
    for (let i2 of e) {
      if (i2.text === `<` ? n2++ : i2.text === `>` ? n2 = Math.max(0, n2 - 1) : i2.text === `(` ? r2++ : i2.text === `)` && (r2 = Math.max(0, r2 - 1)), i2.text === `,` && n2 === 0 && r2 === 0) {
        t4.push([]);
        continue;
      }
      t4[t4.length - 1].push(i2);
    }
    return t4.map(It).filter((e2) => e2.length > 0);
  }
  function It(e) {
    let t4 = 0, n2 = e.length;
    for (; t4 < n2 && e[t4].text === `,`; ) t4++;
    for (; n2 > t4 && e[n2 - 1].text === `,`; ) n2--;
    return e.slice(t4, n2);
  }
  function Lt(e) {
    if (e !== void 0 && Rt(e)) return Number(e.replace(/[ui]$/, ``));
  }
  function Rt(e) {
    return /^(0|[1-9][0-9]*)([ui])?$/.test(e);
  }
  function zt(e) {
    if (e === `read` || e === `write` || e === `read_write`) return e;
  }
  function Bt(e) {
    return [`f32`, `f16`, `i32`, `u32`, `bool`].find((t4) => t4 === e);
  }
  function Vt(e) {
    return { kind: `scalar`, name: e === `f` ? `f32` : e === `h` ? `f16` : e === `i` ? `i32` : `u32` };
  }
  function Ht(e) {
    return e === `f16` ? 2 : 4;
  }
  function Ut(e, t4) {
    return Math.ceil(t4 / e) * e;
  }
  function I2(e) {
    let t4 = It(e);
    if (t4.length === 0) throw j2(`VGPU-WGSL-REFLECT-TYPE`, `Expected WGSL type`);
    let n2 = t4.map((e2) => e2.text).join(``), r2 = Wt(n2);
    if (r2) return r2;
    if (t4[1]?.text === `<`) {
      let e2 = t4[0].text, n3 = Gt(e2, Ft(t4.slice(2, -1)));
      if (n3) return n3;
    }
    return Kt(n2) || qt(n2);
  }
  function Wt(e) {
    let t4 = Bt(e);
    if (t4) return { kind: `scalar`, name: t4 };
    let n2 = e.match(/^vec([234])([fiuh])$/);
    if (n2) return { kind: `vector`, width: Number(n2[1]), element: Vt(n2[2]) };
    let r2 = e.match(/^mat([234])x([234])([fh])$/);
    if (r2) {
      let e2 = r2[3] === `h` ? { kind: `scalar`, name: `f16` } : { kind: `scalar`, name: `f32` };
      return { kind: `matrix`, columns: Number(r2[1]), rows: Number(r2[2]), element: e2 };
    }
  }
  function Gt(e, t4) {
    if (e === `array`) {
      let e2 = t4[1]?.map((e3) => e3.text).join(``), n2 = e2 === void 0 ? void 0 : Lt(e2);
      return { kind: `array`, element: I2(t4[0] ?? []), count: n2, countExpression: e2 };
    }
    if (e === `atomic`) return { kind: `atomic`, element: I2(t4[0] ?? []) };
    if (e === `vec2` || e === `vec3` || e === `vec4`) return { kind: `vector`, width: Number(e.slice(3)), element: I2(t4[0] ?? []) };
    if (/^mat[234]x[234]$/.test(e)) return { kind: `matrix`, columns: Number(e[3]), rows: Number(e[5]), element: I2(t4[0] ?? []) };
    if (e === `ptr`) return { kind: `ptr`, addressSpace: t4[0]?.map((e2) => e2.text).join(``) ?? ``, element: I2(t4[1] ?? []), access: t4[2]?.map((e2) => e2.text).join(``) };
    if (e === `sampler`) return { kind: `sampler`, comparison: false };
    if (e.startsWith(`texture_storage_`)) return { kind: `texture`, textureKind: e, dimension: e.slice(16), texelFormat: t4[0]?.map((e2) => e2.text).join(``), access: zt(t4[1]?.map((e2) => e2.text).join(``)) };
    if (e.startsWith(`texture_`)) return { kind: `texture`, textureKind: e, dimension: e.slice(8), sampleType: t4[0] ? I2(t4[0]) : void 0 };
  }
  function Kt(e) {
    if (e === `sampler` || e === `sampler_comparison`) return { kind: `sampler`, comparison: e === `sampler_comparison` };
    if (e === `texture_external`) return { kind: `texture`, textureKind: e };
    if (e.startsWith(`texture_depth_`) || e.startsWith(`texture_`)) return { kind: `texture`, textureKind: e, dimension: e.slice(8) };
  }
  function qt(e) {
    return { kind: `identifier`, name: e };
  }
  function L2(e) {
    if (e?.kind !== `ident` && e?.kind !== `keyword`) throw j2(`VGPU-WGSL-REFLECT-PARSE`, `Expected identifier`, e?.line, e?.column);
    return e.text;
  }
  function R2(e, t4, n2) {
    for (let r2 = t4; r2 < e.length; r2++) if (e[r2].text === n2) return r2;
    throw j2(`VGPU-WGSL-REFLECT-PARSE`, `Expected ${n2}`, e[t4]?.line, e[t4]?.column);
  }
  function Jt(e, t4, n2, r2) {
    for (let i2 = t4; i2 < n2; i2++) if (e[i2].text === r2) return i2;
  }
  function Yt(e, t4, n2) {
    let r2 = 0;
    for (let i2 = t4; i2 < e.length; i2++) if ((e[i2].text === `{` || e[i2].text === `(`) && r2++, (e[i2].text === `}` || e[i2].text === `)`) && (r2 = Math.max(0, r2 - 1)), r2 === 0 && e[i2].text === n2) return i2;
    return e.length;
  }
  function Xt(e, t4) {
    let n2 = e[t4].text, r2 = n2 === `(` ? `)` : n2 === `{` ? `}` : `>`, i2 = 0;
    for (let a2 = t4; a2 < e.length; a2++) if (e[a2].text === n2 && i2++, e[a2].text === r2 && (i2--, i2 === 0)) return a2;
    throw j2(`VGPU-WGSL-REFLECT-PARSE`, `Unclosed ${n2}`, e[t4]?.line, e[t4]?.column);
  }
  function Zt(e, t4) {
    let n2 = [], r2 = t4;
    for (; e[r2]?.text === `@`; ) {
      let t5 = e[r2], i2 = L2(e[r2 + 1]);
      r2 += 2;
      let a2 = [];
      if (e[r2]?.text === `(`) {
        let t6 = Xt(e, r2);
        a2 = e.slice(r2 + 1, t6), r2 = t6 + 1;
      }
      n2.push({ name: i2, args: a2, token: t5 });
    }
    return [n2, r2];
  }
  function Qt(e) {
    switch (e.kind) {
      case `scalar`:
        return e.name;
      case `identifier`:
        return e.name;
      case `vector`:
        return `vec${e.width}<${Qt(e.element)}>`;
      case `matrix`:
        return `mat${e.columns}x${e.rows}<${Qt(e.element)}>`;
      case `array`:
        return `array<${Qt(e.element)}${e.count === void 0 ? `` : `,${e.count}`}>`;
      default:
        return e.kind;
    }
  }
  function $t(e) {
    let t4 = e.find((e2) => e2.name === `workgroup_size`);
    if (!t4) return;
    let n2 = Ft(t4.args).map((e2) => Number(e2.map((e3) => e3.text).join(``)));
    return [n2[0] ?? 1, n2[1] ?? 1, n2[2] ?? 1];
  }
  function en(e, t4) {
    if (e[t4]?.text !== `<`) return { after: t4 };
    let n2 = R2(e, t4, `>`), r2 = Ft(e.slice(t4 + 1, n2)).map((e2) => e2.map((e3) => e3.text).join(``));
    return { addressSpace: r2[0], access: zt(r2[1]), after: n2 + 1 };
  }
  function tn(e) {
    let t4 = [], n2 = [], r2 = [], i2 = [], a2 = [], o2 = [], s3 = e.tokens.filter((e2) => e2.kind !== `lineComment` && e2.kind !== `blockComment`), c2 = 0, l2 = 0;
    for (; c2 < s3.length; ) {
      let u3 = s3[c2];
      if (u3.text === `{`) {
        l2++, c2++;
        continue;
      }
      if (u3.text === `}`) {
        l2 = Math.max(0, l2 - 1), c2++;
        continue;
      }
      if (l2 > 0) {
        c2++;
        continue;
      }
      let d2 = c2, f2;
      s3[c2]?.text === `export` ? (c2++, [f2, c2] = Zt(s3, c2)) : ([f2, c2] = Zt(s3, c2), s3[c2]?.text === `export` && c2++);
      let p2 = s3[c2]?.text;
      if (p2 === `enable`) {
        s3[c2 + 1]?.kind === `ident` && o2.push(s3[c2 + 1].text), c2 = Yt(s3, c2, `;`) + 1;
        continue;
      }
      if (p2 === `struct`) {
        let n3 = nn(e, s3, c2, f2);
        n3.item && t4.push(n3.item), c2 = n3.next;
        continue;
      }
      if (p2 === `alias`) {
        let t5 = rn(e, s3, c2, f2);
        t5.item && n2.push(t5.item), c2 = t5.next;
        continue;
      }
      if (p2 === `var`) {
        let t5 = an(e, s3, c2, f2);
        t5.item && r2.push(t5.item), c2 = t5.next;
        continue;
      }
      if (p2 === `fn`) {
        let t5 = on(e, s3, c2, f2, d2);
        t5.item && i2.push(t5.item), c2 = t5.next;
        continue;
      }
      if (p2 === `override`) {
        let e2 = cn(s3, c2, f2);
        e2.item && a2.push(e2.item), c2 = e2.next;
        continue;
      }
      c2 = Math.max(d2 + 1, c2 + 1);
    }
    return { structs: t4, aliases: n2, vars: r2, entries: i2, overrides: a2, features: o2 };
  }
  function nn(e, t4, n2, r2) {
    let i2 = L2(t4[n2 + 1]), a2 = R2(t4, n2 + 2, `{`), o2 = Xt(t4, a2);
    return { item: { name: i2, originalName: i2, mangledName: un(e, i2, `struct`), members: ln(t4.slice(a2 + 1, o2)), path: e.path }, next: o2 + 1 };
  }
  function rn(e, t4, n2, r2) {
    let i2 = L2(t4[n2 + 1]), a2 = R2(t4, n2 + 2, `=`), o2 = Yt(t4, a2 + 1, `;`);
    return { item: { name: i2, originalName: i2, mangledName: un(e, i2, `alias`), target: I2(t4.slice(a2 + 1, o2)), path: e.path }, next: o2 + 1 };
  }
  function an(e, t4, n2, r2) {
    let { addressSpace: i2, access: a2, after: o2 } = en(t4, n2 + 1), s3 = L2(t4[o2]), c2 = R2(t4, o2 + 1, `:`), l2 = Yt(t4, c2 + 1, `;`);
    return { item: { path: e.path, name: s3, mangledName: dn(r2) ? s3 : un(e, s3, `var`), attrs: r2, addressSpace: i2, access: a2, type: I2(t4.slice(c2 + 1, l2)) }, next: l2 + 1 };
  }
  function on(e, t4, n2, r2, i2 = n2) {
    let a2 = L2(t4[n2 + 1]), o2 = r2.find((e2) => e2.name === `vertex` || e2.name === `fragment` || e2.name === `compute`)?.name;
    if (!o2) return { item: void 0, next: n2 + 1 };
    let s3 = R2(t4, n2 + 2, `(`), c2 = Xt(t4, s3), l2 = Xt(t4, R2(t4, c2 + 1, `{`));
    return { item: { name: a2, mangledName: a2, stage: o2, workgroupSize: $t(r2), path: e.path, params: sn(t4.slice(s3 + 1, c2)), declarationStartToken: t4[i2], declarationEndToken: t4[l2] }, next: l2 + 1 };
  }
  function sn(e) {
    let t4 = [], n2 = 0;
    for (; n2 < e.length; ) {
      let [r2, i2] = Zt(e, n2);
      if (n2 = i2, !e[n2] || e[n2].text === `,`) {
        n2++;
        continue;
      }
      let a2 = L2(e[n2]), o2 = R2(e, n2 + 1, `:`), s3 = o2 + 1, c2 = 0;
      for (; s3 < e.length && (e[s3].text === `<` && c2++, e[s3].text === `>` && (c2 = Math.max(0, c2 - 1)), !(c2 === 0 && e[s3].text === `,`)); ) s3++;
      t4.push({ name: a2, attrs: r2, type: I2(e.slice(o2 + 1, s3)) }), n2 = s3 + 1;
    }
    return t4;
  }
  function cn(e, t4, n2) {
    let r2 = L2(e[t4 + 1]), i2 = Yt(e, t4 + 1, `;`), a2 = Jt(e, t4 + 2, i2, `=`);
    return { item: { name: r2, mangledName: r2, id: F2(n2, `id`), defaultValue: a2 === void 0 ? void 0 : e.slice(a2 + 1, i2).map((e2) => e2.text).join(``) }, next: i2 + 1 };
  }
  function ln(e) {
    let t4 = [], n2 = 0;
    for (; n2 < e.length; ) {
      let [r2, i2] = Zt(e, n2);
      if (n2 = i2, !e[n2] || e[n2].text === `,` || e[n2].text === `;`) {
        n2++;
        continue;
      }
      let a2 = L2(e[n2]), o2 = R2(e, n2 + 1, `:`), s3 = o2 + 1, c2 = 0;
      for (; s3 < e.length && (e[s3].text === `<` && c2++, e[s3].text === `>` && (c2 = Math.max(0, c2 - 1)), !(c2 === 0 && (e[s3].text === `,` || e[s3].text === `;`))); ) s3++;
      t4.push({ name: a2, attrs: r2, type: I2(e.slice(o2 + 1, s3)), align: F2(r2, `align`), size: F2(r2, `size`) }), n2 = s3 + 1;
    }
    return t4;
  }
  function un(e, t4, n2) {
    return n2 === `override` ? t4 : Pt(e.path, t4);
  }
  function dn(e) {
    return F2(e, `group`) !== void 0 || F2(e, `binding`) !== void 0;
  }
  var fn = `literal length required for auto layout; use draw.group(n, bg) manual binding`;
  var pn = "VGPUError: `bool` is not host-shareable in uniform/storage. Fix: use `u32` (0 | 1) \u2192 struct Params { enabled: u32 }";
  var mn = "use a manual group claim (`draw.group(n, bg)`)";
  function hn(e = 1, t4 = 1) {
    return j2(`VGPU-WGSL-REFLECT-ARRAY-LENGTH`, fn, e, t4);
  }
  function gn(e = 1, t4 = 1) {
    return j2(`VGPU-WGSL-REFLECT-BOOL-HOST-SHAREABLE`, pn, e, t4);
  }
  function _n2(e, t4, n2 = 1, r2 = 1) {
    return j2(`VGPU-WGSL-REFLECT-UNKNOWN-TYPE`, `type '${e}' is unknown in ${t4}; ${mn}`, n2, r2);
  }
  function vn(e, t4, n2 = 1, r2 = 1) {
    return j2(`VGPU-WGSL-REFLECT-NS-TYPE`, `type '${e}' is a namespace-member import; use a named import or manual @group(1+) binding`, n2, r2);
  }
  function yn(e, t4 = 1, n2 = 1) {
    return j2(`VGPU-WGSL-REFLECT-NON-HOST-SHAREABLE`, `Type ${e} is not host-shareable; ${mn}`, t4, n2);
  }
  var bn = `wgsl-host-shareable-v1`;
  function xn(e, t4, n2) {
    let r2 = /* @__PURE__ */ new Map();
    for (let e2 of t4) {
      let t5 = /* @__PURE__ */ new Map();
      for (let n3 of [...e2.structs, ...e2.aliases]) t5.set(n3.originalName, { path: n3.path, name: n3.originalName, mangledName: n3.mangledName, kind: `members` in n3 ? `struct` : `alias` });
      r2.set(e2.structs[0]?.path ?? e2.aliases[0]?.path ?? e2.vars[0]?.path ?? ``, t5);
    }
    let i2 = new Map(e.map((e2) => [e2.path, r2.get(e2.path) ?? /* @__PURE__ */ new Map()])), a2 = /* @__PURE__ */ new Map();
    for (let t5 of e) {
      let r3 = new Map(i2.get(t5.path));
      for (let a3 of t5.parsed.imports) Sn(t5, a3, r3, e, i2, n2);
      a2.set(t5.path, r3);
    }
    return a2;
  }
  function Sn(e, t4, n2, r2, i2, a2) {
    let o2 = En(t4, e.path, r2, a2), s3 = i2.get(o2);
    for (let e2 of t4.bindings) {
      if (e2.namespace) {
        n2.set(e2.local, { path: o2, name: e2.local, mangledName: e2.local, kind: `namespace` });
        continue;
      }
      let t5 = s3?.get(e2.imported);
      t5 && n2.set(e2.local, t5);
    }
  }
  function Cn(e, t4) {
    let n2 = /* @__PURE__ */ new Map(), r2 = /* @__PURE__ */ new Map(), i2 = /* @__PURE__ */ new Map(), a2 = { structs: n2, aliases: r2, byMangled: i2 };
    for (let o2 of e) {
      for (let e2 of o2.structs) {
        let r3 = { name: e2.name, mangledName: e2.mangledName, members: e2.members.map((n3) => ({ name: n3.name, type: z2(n3.type, e2.path, t4, a2), align: n3.align, size: n3.size })) };
        n2.set(e2.mangledName, r3), i2.set(e2.mangledName, r3);
      }
      for (let e2 of o2.aliases) {
        let n3 = { name: e2.name, mangledName: e2.mangledName, target: z2(e2.target, e2.path, t4, a2) };
        r2.set(e2.mangledName, n3), i2.set(e2.mangledName, n3);
      }
    }
    return { structs: n2, aliases: r2, byMangled: i2 };
  }
  function z2(e, t4, n2, r2) {
    switch (e.kind) {
      case `identifier`: {
        let r3 = e.name.indexOf(`.`);
        if (r3 > 0) {
          let i3 = e.name.slice(0, r3);
          if (n2.get(t4)?.get(i3)?.kind === `namespace`) throw vn(e.name, t4);
        }
        let i2 = n2.get(t4)?.get(e.name);
        if (i2?.kind === `namespace`) throw vn(e.name, t4);
        if (!i2) throw _n2(e.name, t4);
        return { kind: `identifier`, name: i2.name, mangledName: i2.mangledName };
      }
      case `array`:
      case `atomic`:
      case `vector`:
      case `matrix`:
      case `ptr`:
        return { ...e, element: z2(e.element, t4, n2, r2) };
      case `texture`:
        return { ...e, sampleType: e.sampleType ? z2(e.sampleType, t4, n2, r2) : void 0 };
      default:
        return e;
    }
  }
  function wn(e, t4) {
    if (!t4 || e.kind !== `identifier`) return e;
    let n2 = t4.aliases.get(e.mangledName ?? e.name);
    return n2 ? wn(n2.target, t4) : e;
  }
  function Tn(e, t4) {
    let n2 = wn(e, t4);
    switch (n2.kind) {
      case `array`:
      case `atomic`:
      case `vector`:
      case `matrix`:
      case `ptr`:
        return { ...n2, element: Tn(n2.element, t4) };
      case `texture`:
        return { ...n2, sampleType: n2.sampleType ? Tn(n2.sampleType, t4) : void 0 };
      default:
        return n2;
    }
  }
  function En(e, t4, n2, r2) {
    let i2 = Dn(e, t4, r2);
    if (i2 !== void 0 && n2.some((e2) => e2.path === i2)) return i2;
    let a2 = e.from, o2 = t4.slice(0, t4.lastIndexOf(`/`) + 1), s3 = a2.startsWith(`/`) ? a2 : On(`${o2}${a2}`);
    return [a2, s3].find((e2) => n2.some((t5) => t5.path === e2)) ?? i2 ?? s3;
  }
  function Dn(e, t4, n2) {
    if (n2) try {
      return n2(t4, e);
    } catch {
      return;
    }
  }
  function On(e) {
    let t4 = e.startsWith(`/`), n2 = [];
    for (let t5 of e.split(`/`)) !t5 || t5 === `.` || (t5 === `..` ? n2.pop() : n2.push(t5));
    return `${t4 ? `/` : ``}${n2.join(`/`)}`;
  }
  function kn(e, t4 = Qt(e), n2 = t4, r2) {
    return An(r2 ? Tn(e, r2) : e, t4, n2, r2);
  }
  function An(e, t4, n2, r2) {
    switch (e.kind) {
      case `scalar`:
        return jn(e, t4, n2);
      case `atomic`:
        return Mn(e, t4, n2);
      case `vector`:
        return Nn(e, t4, n2, r2);
      case `matrix`:
        return Pn(e, t4, n2, r2);
      case `array`:
        return Fn(e, t4, n2, r2);
      case `identifier`:
        return Ln(e, t4, n2, r2);
      default:
        throw yn(Qt(e));
    }
  }
  function jn(e, t4, n2) {
    let r2 = Ht(e.name);
    if (e.name === `bool`) throw gn();
    return { name: t4, mangledName: n2, layoutMode: bn, type: e, align: r2, size: r2 };
  }
  function Mn(e, t4, n2) {
    return { name: t4, mangledName: n2, layoutMode: bn, type: e, align: 4, size: 4 };
  }
  function Nn(e, t4, n2, r2) {
    let i2 = kn(e.element, t4, n2, r2).size ?? 4;
    return { name: t4, mangledName: n2, layoutMode: bn, type: e, align: e.width === 2 ? i2 * 2 : i2 * 4, size: i2 * e.width };
  }
  function Pn(e, t4, n2, r2) {
    let i2 = kn({ kind: `vector`, width: e.rows, element: e.element }, `${t4}[]`, `${n2}[]`, r2), a2 = Ut(i2.align, i2.size ?? 0);
    return { name: t4, mangledName: n2, layoutMode: bn, type: e, align: i2.align, size: a2 * e.columns, stride: a2, element: i2 };
  }
  function Fn(e, t4, n2, r2) {
    In(e.countExpression);
    let i2 = kn(e.element, `${t4}[]`, `${n2}[]`, r2), a2 = Ut(B2(e.element, r2), i2.size ?? 0);
    return { name: t4, mangledName: n2, layoutMode: bn, type: e, align: B2(e, r2), size: e.count === void 0 ? void 0 : a2 * e.count, stride: a2, element: i2, runtimeSized: e.count === void 0 };
  }
  function In(e) {
    if (e !== void 0 && !Rt(e)) throw hn();
  }
  function Ln(e, t4, n2, r2) {
    if (!r2) throw _n2(e.name, `<unknown>`);
    let i2 = r2.structs.get(e.mangledName ?? e.name);
    if (!i2) throw _n2(e.name, `<unknown>`);
    let a2 = [], o2 = 0, s3 = 1;
    for (let e2 of i2.members) {
      let t5 = Rn(e2, o2, r2);
      a2.push(t5.member), o2 = t5.offset + (t5.member.size ?? 0), s3 = Math.max(s3, t5.member.align);
    }
    return { name: t4, mangledName: n2, layoutMode: bn, type: e, align: s3, size: Ut(s3, o2), members: a2 };
  }
  function Rn(e, t4, n2) {
    let r2 = kn(e.type, e.name, e.name, n2), i2 = Math.max(B2(e.type, n2), e.align ?? 1), a2 = Math.max(r2.size ?? 0, e.size ?? 0), o2 = Ut(i2, t4);
    return { member: { name: e.name, offset: o2, align: i2, size: a2, type: e.type, layout: r2, explicitAlign: e.align, explicitSize: e.size }, offset: o2 };
  }
  function B2(e, t4) {
    let n2 = t4 ? Tn(e, t4) : e;
    switch (n2.kind) {
      case `scalar`:
        return zn(n2.name);
      case `atomic`:
        return 4;
      case `vector`:
        return n2.width === 2 ? B2(n2.element, t4) * 2 : B2(n2.element, t4) * 4;
      case `matrix`:
        return B2({ kind: `vector`, width: n2.rows, element: n2.element }, t4);
      case `array`:
        return B2(n2.element, t4);
      case `identifier`:
        return Bn(n2, t4);
      default:
        throw yn(Qt(n2));
    }
  }
  function zn(e) {
    if (e === `bool`) throw gn();
    return Ht(e);
  }
  function Bn(e, t4) {
    let n2 = t4?.structs.get(e.mangledName ?? e.name);
    if (!n2) throw _n2(e.name, `<unknown>`);
    return Math.max(1, ...n2.members.map((e2) => Math.max(B2(e2.type, t4), e2.align ?? 1)));
  }
  var Vn = /^_vgsl_[0-9a-f]{8,16}__[A-Za-z_][A-Za-z0-9_]*$/;
  var Hn = /* @__PURE__ */ new Set([`fn`, `struct`, `const`, `alias`, `var`, `override`]);
  function Un(e) {
    return new Wn(e).analyze();
  }
  var Wn = class {
    constructor(e) {
      __publicField(this, "tokens");
      __publicField(this, "scopes", []);
      __publicField(this, "declarations", []);
      __publicField(this, "references", []);
      __publicField(this, "functions", []);
      __publicField(this, "preserved", /* @__PURE__ */ new Map());
      __publicField(this, "symbolsByScope", /* @__PURE__ */ new Map());
      __publicField(this, "moduleFallbackReasons", []);
      __publicField(this, "pendingSymbols", []);
      __publicField(this, "moduleScopeId");
      this.tokens = e, this.moduleScopeId = this.createScope(`module`, void 0, void 0, 0);
    }
    analyze() {
      this.collectTopLevel();
      for (let e of this.functions) this.walkFunction(e);
      return { tokens: this.tokens, scopes: this.scopes, declarations: this.declarations, references: this.references, functions: this.functions, preservedTokens: [...this.preserved.entries()].map(([e, t4]) => ({ tokenIndex: e, reason: t4 })), fallback: { wholeModule: this.moduleFallbackReasons.length > 0, reasons: this.moduleFallbackReasons } };
    }
    collectTopLevel() {
      let e = 0;
      for (let t4 = 0; t4 < this.tokens.length; t4++) {
        let n2 = this.tokens[t4];
        if (!V2(n2)) {
          if (n2.text === `{`) {
            e++;
            continue;
          }
          if (n2.text === `}`) {
            e--, e < 0 && (this.moduleFallback(`unmatched top-level closing brace`, t4), e = 0);
            continue;
          }
          if (e === 0) {
            if (n2.text === `@`) {
              t4 = this.preserveAttribute(t4);
              continue;
            }
            if (n2.text === `enable` || n2.text === `requires` || n2.text === `diagnostic` || n2.text === `const_assert`) {
              t4 = this.preserveStatement(t4, `directive`);
              continue;
            }
            if (n2.text !== `export`) {
              if (n2.text === `struct`) {
                t4 = this.collectStruct(t4);
                continue;
              }
              if (n2.text === `fn`) {
                t4 = this.collectFunction(t4);
                continue;
              }
              if (n2.text === `const` || n2.text === `alias` || n2.text === `var` || n2.text === `override`) {
                t4 = this.preserveGlobalDeclaration(t4);
                continue;
              }
              n2.kind === `keyword` && !Hn.has(n2.text) && this.moduleFallback(`unexpected top-level keyword '${n2.text}'`, t4);
            }
          }
        }
      }
      e !== 0 && this.moduleFallback(`unclosed top-level brace`, this.tokens.length - 1), this.scopes[this.moduleScopeId].endToken = Math.max(0, this.tokens.length - 1);
    }
    collectStruct(e) {
      let t4 = this.nextSig(e);
      if (t4 === void 0 || this.tokens[t4]?.kind !== `ident`) return this.moduleFallback(`struct without name`, e), e;
      this.preserveToken(t4, `global`);
      let n2 = this.nextSig(t4);
      if (n2 === void 0 || this.tokens[n2]?.text !== `{`) return this.moduleFallback(`struct without body`, e), t4;
      let r2 = this.findMatching(n2, `{`, `}`);
      if (r2 === void 0) return this.moduleFallback(`unclosed struct body`, n2), n2;
      for (let e2 = n2; e2 <= r2; e2++) this.tokens[e2]?.kind === `ident` && this.preserveToken(e2, `struct`);
      return r2;
    }
    collectFunction(e) {
      let t4 = this.nextSig(e);
      if (t4 === void 0 || this.tokens[t4]?.kind !== `ident`) return this.moduleFallback(`function without name`, e), e;
      let n2 = this.tokens[t4].text, r2 = Vn.test(n2) && !this.hasEntryAttributeBefore(e);
      this.addDeclaration(n2, `function`, t4, this.moduleScopeId, void 0, r2), r2 || this.preserveToken(t4, `global`);
      let i2 = this.nextSig(t4);
      if (i2 === void 0 || this.tokens[i2]?.text !== `(`) return this.moduleFallback(`function without parameter list`, t4), t4;
      let a2 = this.findMatching(i2, `(`, `)`);
      if (a2 === void 0) return this.moduleFallback(`unclosed function parameter list`, i2), i2;
      let o2 = this.findNextText(a2 + 1, `{`);
      if (o2 === void 0) return this.moduleFallback(`function without body`, a2), a2;
      this.preserveFunctionSignatureTail(a2 + 1, o2);
      let s3 = this.findMatching(o2, `{`, `}`);
      if (s3 === void 0) return this.moduleFallback(`unclosed function body`, o2), o2;
      let c2 = this.createScope(`function`, this.moduleScopeId, this.functions.length, i2);
      return this.functions.push({ id: this.functions.length, name: n2, nameTokenIndex: t4, scopeId: c2, bodyStartToken: o2, bodyEndToken: s3, skipped: false, fallbackReasons: [] }), this.collectParams(i2, a2, c2, this.functions.length - 1), this.scopes[c2].endToken = s3, s3;
    }
    collectParams(e, t4, n2, r2) {
      for (let i2 = e + 1; i2 < t4; i2++) {
        let e2 = this.tokens[i2];
        if (!V2(e2)) {
          if (e2.text === `@`) {
            i2 = this.preserveAttribute(i2);
            continue;
          }
          if (e2.kind === `ident` && this.nextSig(i2) !== void 0 && this.tokens[this.nextSig(i2)]?.text === `:`) {
            this.addDeclaration(e2.text, `param`, i2, n2, r2, true);
            let a2 = this.nextSig(i2);
            i2 = this.preserveTypeFrom(a2 + 1, [`,`, `)`], t4);
          }
        }
      }
    }
    preserveFunctionSignatureTail(e, t4) {
      for (let n2 = e; n2 < t4; n2++) {
        let e2 = this.tokens[n2];
        if (!V2(e2)) {
          if (e2.text === `@`) {
            n2 = this.preserveAttribute(n2);
            continue;
          }
          e2.kind === `ident` && this.preserveToken(n2, `type`);
        }
      }
    }
    preserveGlobalDeclaration(e) {
      let t4 = e + 1;
      if (this.tokens[e]?.text === `var`) {
        let n3 = this.nextSig(e);
        if (n3 !== void 0 && this.tokens[n3]?.text === `<`) {
          let e2 = this.findMatching(n3, `<`, `>`);
          if (e2 === void 0) return this.moduleFallback(`unparseable top-level var template`, n3), n3;
          this.preserveRange(n3, e2, `type`), t4 = e2 + 1;
        }
      }
      let n2 = this.findNextIdent(t4);
      n2 !== void 0 && (this.preserveToken(n2, `global`), this.addDeclaration(this.tokens[n2].text, `global`, n2, this.moduleScopeId, void 0, false));
      let r2 = this.findStatementEnd(e);
      for (let t5 = e; t5 <= r2; t5++) this.tokens[t5]?.kind === `ident` && this.preserveToken(t5, `global`);
      return r2;
    }
    walkFunction(e) {
      let t4 = [this.moduleScopeId, e.scopeId], n2 = [], r2 = (n3, r3) => {
        let i3 = this.createScope(n3, t4[t4.length - 1], e.id, r3);
        return t4.push(i3), i3;
      }, i2 = (n3) => {
        if (t4.length <= 2) {
          this.functionFallback(e, `scope frame underflow`, n3);
          return;
        }
        let r3 = t4.pop();
        return this.scopes[r3].endToken = n3, r3;
      };
      r2(`block`, e.bodyStartToken);
      let a2 = 1;
      for (let o2 = e.bodyStartToken + 1; o2 < e.bodyEndToken; o2++) {
        this.activatePendingSymbols(o2);
        let s3 = this.tokens[o2];
        if (V2(s3)) continue;
        if (s3.text === `@`) {
          o2 = this.preserveAttribute(o2);
          continue;
        }
        if (s3.text === `.`) {
          let e2 = this.nextSig(o2);
          e2 !== void 0 && this.tokens[e2]?.kind === `ident` && this.preserveToken(e2, `member`);
          continue;
        }
        if (s3.text === `enable` || s3.text === `requires` || s3.text === `diagnostic`) {
          o2 = this.preserveStatement(o2, `directive`);
          continue;
        }
        if (s3.text === `for`) {
          let t5 = r2(`for-init`, o2), i3 = this.nextSig(o2);
          (i3 === void 0 || this.tokens[i3]?.text !== `(`) && this.functionFallback(e, `for without parenthesized header`, o2), n2.push({ scopeId: t5, headerDepth: 0, awaitingBody: false });
          continue;
        }
        let c2 = n2[n2.length - 1];
        if (c2 && c2.bodyDepth === void 0 && (s3.text === `(` && c2.headerDepth++, s3.text === `)` && (c2.headerDepth--, c2.headerDepth <= 0 && (c2.awaitingBody = true))), s3.text === `{`) {
          a2++;
          let e2 = Gn(n2, (e3) => e3.awaitingBody && e3.bodyDepth === void 0);
          e2 && (e2.bodyDepth = a2), r2(`block`, o2);
          continue;
        }
        if (s3.text === `}`) {
          let t5 = a2;
          for (i2(o2), a2--; n2.length > 0 && n2[n2.length - 1].bodyDepth === t5; ) i2(o2), n2.pop();
          a2 < 0 && this.functionFallback(e, `unmatched closing brace`, o2);
          continue;
        }
        if (s3.text === `:`) {
          o2 = this.preserveTypeFrom(o2 + 1, [`=`, `;`, `,`, `)`, `{`], e.bodyEndToken);
          continue;
        }
        if (s3.text === `-` && this.tokens[this.nextSig(o2) ?? -1]?.text === `>`) {
          o2 = this.preserveTypeFrom((this.nextSig(o2) ?? o2) + 1, [`{`], e.bodyEndToken);
          continue;
        }
        if (s3.text === `let` || s3.text === `const` || s3.text === `var`) {
          o2 = this.collectLocalDeclaration(o2, t4[t4.length - 1], e);
          continue;
        }
        if (s3.kind === `ident` && !this.preserved.has(o2)) {
          let n3 = this.resolve(s3.text, t4);
          n3 === void 0 ? this.preserveToken(o2, `unknown`) : this.references.push({ name: s3.text, tokenIndex: o2, declarationId: n3, scopeId: t4[t4.length - 1], functionId: e.id });
        }
      }
      for (; t4.length > 2; ) i2(e.bodyEndToken);
    }
    collectLocalDeclaration(e, t4, n2) {
      let r2 = this.tokens[e].text, i2 = e + 1;
      if (r2 === `var`) {
        let t5 = this.nextSig(e);
        if (t5 !== void 0 && this.tokens[t5]?.text === `<`) {
          let e2 = this.findMatching(t5, `<`, `>`);
          if (e2 === void 0) return this.functionFallback(n2, `unparseable var template`, t5), t5;
          this.preserveRange(t5, e2, `type`), i2 = e2 + 1;
        }
      }
      let a2 = this.findNextIdent(i2);
      if (a2 === void 0 || a2 >= n2.bodyEndToken) return this.functionFallback(n2, `${r2} without identifier`, e), e;
      this.addDeclaration(this.tokens[a2].text, r2, a2, t4, n2.id, true, this.findStatementEnd(e));
      let o2 = this.nextSig(a2);
      return o2 !== void 0 && this.tokens[o2]?.text === `:` ? this.preserveTypeFrom(o2 + 1, [`=`, `;`, `,`, `)`], n2.bodyEndToken) : a2;
    }
    addDeclaration(e, t4, n2, r2, i2, a2, o2) {
      let s3 = this.declarations.length;
      return this.declarations.push({ id: s3, name: e, kind: t4, tokenIndex: n2, scopeId: r2, functionId: i2, safeToRename: a2 }), o2 === void 0 ? this.activateSymbol(e, s3, r2) : this.pendingSymbols.push({ name: e, id: s3, scopeId: r2, activateAfter: o2 }), s3;
    }
    activatePendingSymbols(e) {
      for (let t4 = this.pendingSymbols.length - 1; t4 >= 0; t4--) {
        let n2 = this.pendingSymbols[t4];
        n2.activateAfter >= e || (this.activateSymbol(n2.name, n2.id, n2.scopeId), this.pendingSymbols.splice(t4, 1));
      }
    }
    activateSymbol(e, t4, n2) {
      let r2 = this.symbolsByScope.get(n2);
      r2 || (r2 = /* @__PURE__ */ new Map(), this.symbolsByScope.set(n2, r2)), r2.has(e) || r2.set(e, t4);
    }
    resolve(e, t4) {
      for (let n2 = t4.length - 1; n2 >= 0; n2--) {
        let r2 = this.symbolsByScope.get(t4[n2])?.get(e);
        if (r2 !== void 0) return r2;
      }
    }
    preserveAttribute(e) {
      this.preserveToken(e, `attribute`);
      let t4 = this.nextSig(e);
      if (t4 === void 0) return e;
      this.preserveToken(t4, `attribute`);
      let n2 = this.nextSig(t4);
      if (n2 === void 0 || this.tokens[n2]?.text !== `(`) return t4;
      let r2 = this.findMatching(n2, `(`, `)`);
      return r2 === void 0 ? (this.preserveRange(n2, n2, `attribute`), n2) : (this.preserveRange(n2, r2, `attribute`), r2);
    }
    preserveTypeFrom(e, t4, n2) {
      let r2 = 0, i2 = 0, a2 = 0, o2 = e - 1;
      for (let s3 = e; s3 < n2; s3++) {
        let n3 = this.tokens[s3];
        if (!V2(n3)) {
          if (r2 === 0 && i2 === 0 && a2 === 0 && t4.includes(n3.text)) return Math.max(e - 1, s3 - 1);
          if (n3.text === `<`) r2++;
          else if (n3.text === `>`) r2 = Math.max(0, r2 - 1);
          else if (n3.text === `(`) i2++;
          else if (n3.text === `)`) {
            if (i2 === 0 && t4.includes(`)`)) return Math.max(e - 1, s3 - 1);
            i2 = Math.max(0, i2 - 1);
          } else n3.text === `[` ? a2++ : n3.text === `]` && (a2 = Math.max(0, a2 - 1));
          n3.kind === `ident` && this.preserveToken(s3, `type`), o2 = s3;
        }
      }
      return o2;
    }
    preserveStatement(e, t4) {
      let n2 = this.findStatementEnd(e);
      return this.preserveRange(e, n2, t4), n2;
    }
    preserveRange(e, t4, n2) {
      for (let r2 = e; r2 <= t4; r2++) this.tokens[r2] && this.tokens[r2].kind !== `lineComment` && this.tokens[r2].kind !== `blockComment` && this.preserveToken(r2, n2);
    }
    preserveToken(e, t4) {
      this.preserved.has(e) || this.preserved.set(e, t4);
    }
    createScope(e, t4, n2, r2) {
      let i2 = this.scopes.length;
      return this.scopes.push({ id: i2, kind: e, parentId: t4, functionId: n2, startToken: r2 }), i2;
    }
    nextSig(e) {
      for (let t4 = e + 1; t4 < this.tokens.length; t4++) if (!V2(this.tokens[t4])) return t4;
    }
    findNextIdent(e) {
      for (let t4 = e; t4 < this.tokens.length; t4++) {
        let e2 = this.tokens[t4];
        if (!V2(e2)) {
          if (e2.kind === `ident`) return t4;
          if (e2.text !== `@`) return;
        }
      }
    }
    findNextText(e, t4) {
      for (let n2 = e; n2 < this.tokens.length; n2++) if (!V2(this.tokens[n2]) && this.tokens[n2].text === t4) return n2;
    }
    findStatementEnd(e) {
      let t4 = 0;
      for (let n2 = e; n2 < this.tokens.length; n2++) {
        let e2 = this.tokens[n2].text;
        if (e2 === `(`) t4++;
        else if (e2 === `)`) t4 = Math.max(0, t4 - 1);
        else if (t4 === 0 && (e2 === `;` || e2 === `{` || e2 === `}`)) return n2;
      }
      return this.tokens.length - 1;
    }
    findMatching(e, t4, n2) {
      let r2 = 0;
      for (let i2 = e; i2 < this.tokens.length; i2++) {
        let e2 = this.tokens[i2].text;
        if (e2 === t4 && r2++, e2 === n2 && (r2--, r2 === 0)) return i2;
      }
    }
    hasEntryAttributeBefore(e) {
      for (let t4 = e - 1; t4 >= 0; t4--) {
        let e2 = this.tokens[t4];
        if (!V2(e2)) {
          if (e2.text === `)` || e2.kind === `ident` || e2.text === `@`) {
            let t5 = e2.text;
            if (t5 === `compute` || t5 === `vertex` || t5 === `fragment`) return true;
            continue;
          }
          break;
        }
      }
      return false;
    }
    moduleFallback(e, t4) {
      this.moduleFallbackReasons.push(`${e} at token ${t4}`);
    }
    functionFallback(e, t4, n2) {
      e.skipped = true, e.fallbackReasons.push(`${t4} at token ${n2}`);
    }
  };
  function Gn(e, t4) {
    for (let n2 = e.length - 1; n2 >= 0; n2--) if (t4(e[n2])) return e[n2];
  }
  function V2(e) {
    return e.kind === `lineComment` || e.kind === `blockComment`;
  }
  var Kn = /* @__PURE__ */ new Set([`textureSample`, `textureSampleBias`, `textureSampleLevel`, `textureSampleGrad`, `textureGather`, `textureSampleBaseClampToEdge`]);
  var qn = /* @__PURE__ */ new Set([`textureSampleCompare`, `textureSampleCompareLevel`, `textureGatherCompare`]);
  function Jn(e, t4, n2) {
    let r2 = /* @__PURE__ */ new Map();
    for (let i2 = 0; i2 < e.length; i2++) {
      let a2 = e[i2], o2 = t4[i2], s3 = Un(a2.tokens), c2 = /* @__PURE__ */ new Map();
      for (let e2 of o2.vars) {
        let t5 = F2(e2.attrs, `group`), n3 = F2(e2.attrs, `binding`), r3 = s3.declarations.find((t6) => t6.kind === `global` && t6.name === e2.name);
        t5 !== void 0 && n3 !== void 0 && r3 && c2.set(r3.id, { group: t5, binding: n3 });
      }
      let l2 = /* @__PURE__ */ new Map();
      for (let e2 of s3.declarations) {
        if (e2.kind !== `function`) continue;
        let t5 = s3.functions.find((t6) => t6.nameTokenIndex === e2.tokenIndex);
        t5 && l2.set(e2.id, t5.id);
      }
      for (let e2 of o2.entries) {
        let t5 = s3.functions.find((t6) => t6.name === e2.name), i3 = [], a3 = s3.fallback.wholeModule || !t5;
        !a3 && t5 && (a3 = !Yn(t5.id, /* @__PURE__ */ new Map(), /* @__PURE__ */ new Set(), s3, c2, l2, i3));
        let o3 = t5 ? er(t5.id, s3, c2, l2) : n2.map(nr);
        r2.set(e2, a3 ? tr(n2, o3) : rr(i3));
      }
    }
    return r2;
  }
  function Yn(e, t4, n2, r2, i2, a2, o2) {
    let s3 = r2.functions[e];
    if (!s3 || s3.skipped) return false;
    let c2 = `${e}|${[...t4].map(([e2, t5]) => `${e2}:${t5.group}:${t5.binding}`).join(`,`)}`;
    if (n2.has(c2)) return true;
    n2.add(c2);
    let l2 = r2.references.filter((t5) => t5.functionId === e), u3 = new Map(l2.map((e2) => [e2.tokenIndex, e2]));
    for (let e2 = s3.bodyStartToken + 1; e2 < s3.bodyEndToken; e2++) {
      let s4 = r2.tokens[e2]?.text, c3 = Kn.has(s4 ?? ``) ? `filtering` : qn.has(s4 ?? ``) ? `comparison` : void 0, l3 = u3.get(e2), d2 = l3 && a2.get(l3.declarationId);
      if (!c3 && d2 === void 0) continue;
      let f2 = $n(r2, e2);
      if (f2 === void 0 || r2.tokens[f2]?.text !== `(`) continue;
      let p2 = Qn(r2, f2);
      if (!p2) return false;
      let m2 = p2.map(([e3, n3]) => Xn(e3, n3, r2, i2, t4));
      if (c3) {
        let e3 = +(s4 === `textureGather` && !Zn(p2[0], r2, i2, t4)), n3 = m2[e3], a3 = m2[e3 + 1];
        if (!n3 || !a3) return false;
        o2.push({ texture: n3, sampler: a3, mode: c3 });
      } else {
        let e3 = r2.declarations.filter((e4) => e4.kind === `param` && e4.functionId === d2).sort((e4, t6) => e4.tokenIndex - t6.tokenIndex), t5 = /* @__PURE__ */ new Map();
        for (let n3 = 0; n3 < e3.length; n3++) m2[n3] && t5.set(e3[n3].id, m2[n3]);
        if (!Yn(d2, t5, n2, r2, i2, a2, o2)) return false;
      }
    }
    return true;
  }
  function Xn(e, t4, n2, r2, i2) {
    for (let a2 of n2.references) {
      if (a2.tokenIndex < e || a2.tokenIndex > t4) continue;
      let n3 = r2.get(a2.declarationId) ?? i2.get(a2.declarationId);
      if (n3) return n3;
    }
  }
  function Zn(e, t4, n2, r2) {
    let i2 = t4.references.find((t5) => t5.tokenIndex >= e[0] && t5.tokenIndex <= e[1]);
    return i2?.tokenIndex === e[0] ? n2.get(i2.declarationId) ?? r2.get(i2.declarationId) : void 0;
  }
  function Qn(e, t4) {
    let n2 = [], r2 = 1, i2 = 0, a2 = 0, o2 = 0, s3 = t4 + 1;
    for (let c2 = t4 + 1; c2 < e.tokens.length; c2++) {
      let t5 = e.tokens[c2].text;
      if (t5 === `(`) r2++;
      else if (t5 === `)`) {
        if (r2--, r2 === 0) return n2.push([s3, c2 - 1]), n2;
      } else t5 === `[` ? i2++ : t5 === `]` ? i2-- : t5 === `{` ? a2++ : t5 === `}` ? a2-- : t5 === `<` ? o2++ : t5 === `>` ? o2-- : t5 === `,` && r2 === 1 && i2 === 0 && a2 === 0 && o2 === 0 && (n2.push([s3, c2 - 1]), s3 = c2 + 1);
    }
  }
  function $n(e, t4) {
    for (let n2 = t4 + 1; n2 < e.tokens.length; n2++) if (e.tokens[n2].kind !== `lineComment` && e.tokens[n2].kind !== `blockComment`) return n2;
  }
  function er(e, t4, n2, r2) {
    let i2 = [e], a2 = /* @__PURE__ */ new Set(), o2 = /* @__PURE__ */ new Map();
    for (; i2.length; ) {
      let e2 = i2.pop();
      if (!a2.has(e2)) {
        a2.add(e2);
        for (let a3 of t4.references) {
          if (a3.functionId !== e2) continue;
          let t5 = n2.get(a3.declarationId);
          t5 && o2.set(`${t5.group}:${t5.binding}`, t5);
          let s3 = r2.get(a3.declarationId);
          s3 !== void 0 && i2.push(s3);
        }
      }
    }
    return [...o2.values()];
  }
  function tr(e, t4) {
    let n2 = new Set(t4.map((e2) => `${e2.group}:${e2.binding}`)), r2 = e.filter((e2) => n2.has(`${e2.group}:${e2.binding}`)), i2 = r2.filter((e2) => e2.bindingLayout?.kind === `texture` && e2.bindingLayout.texture.sampleType === `unfilterable-float` && !e2.bindingLayout.texture.multisampled), a2 = r2.filter((e2) => e2.bindingLayout?.kind === `sampler` && e2.bindingLayout.sampler.type === `filtering`);
    return i2.flatMap((e2) => a2.map((t5) => ({ texture: nr(e2), sampler: nr(t5), mode: `filtering` })));
  }
  function nr(e) {
    return { group: e.group, binding: e.binding };
  }
  function rr(e) {
    let t4 = /* @__PURE__ */ new Set();
    return e.filter((e2) => {
      let n2 = `${e2.texture.group}:${e2.texture.binding}:${e2.sampler.group}:${e2.sampler.binding}:${e2.mode}`;
      return t4.has(n2) ? false : (t4.add(n2), true);
    });
  }
  function ir(e, t4) {
    let n2 = e.map(tn), r2 = xn(e, n2, t4), i2 = Cn(n2, r2), a2 = [], o2 = [];
    for (let e2 of n2) for (let t5 of e2.vars) {
      let e3 = F2(t5.attrs, `group`), n3 = F2(t5.attrs, `binding`);
      if (e3 === void 0 || n3 === void 0) continue;
      let s4 = z2(t5.type, t5.path, r2, i2), c3 = ct2(s4, t5.addressSpace), l2 = t5.addressSpace === `uniform` || t5.addressSpace === `storage` ? kn(s4, t5.name, t5.mangledName, i2) : void 0;
      l2 && o2.push(l2), a2.push({ group: e3, binding: n3, name: t5.name, mangledName: t5.mangledName, type: s4, kind: c3, addressSpace: t5.addressSpace, access: t5.access, struct: s4.kind === `identifier` ? i2.structs.get(s4.mangledName ?? s4.name) : void 0, layout: l2, bindingLayout: lt2(c3, t5.addressSpace, t5.access, s4, l2) });
    }
    a2.sort((e2, t5) => e2.group - t5.group || e2.binding - t5.binding);
    let s3 = ar(e, n2, a2), c2 = Jn(e, n2, a2);
    return { bindings: a2, entryPoints: n2.flatMap((e2) => e2.entries.map((e3) => or(e3, n2.flatMap((e4) => e4.structs), r2, i2, s3.get(e3) ?? a2, c2.get(e3) ?? []))), overrides: n2.flatMap((e2) => e2.overrides), featuresRequired: [...new Set(n2.flatMap((e2) => e2.features))], aliases: [...i2.aliases.values()], structs: [...i2.structs.values()], hostShareableLayouts: o2 };
  }
  function ar(e, t4, n2) {
    let r2 = /* @__PURE__ */ new Map();
    for (let i2 = 0; i2 < e.length; i2++) {
      let a2 = e[i2], o2 = t4[i2], s3 = Un(a2.tokens), c2 = s3.fallback.wholeModule, l2 = /* @__PURE__ */ new Map();
      for (let e2 of s3.declarations) {
        if (e2.kind !== `function`) continue;
        let t5 = s3.functions.find((t6) => t6.nameTokenIndex === e2.tokenIndex);
        t5 && l2.set(e2.id, t5.id);
      }
      let u3 = /* @__PURE__ */ new Map();
      for (let e2 of o2.vars) {
        let t5 = F2(e2.attrs, `group`), n3 = F2(e2.attrs, `binding`);
        if (t5 === void 0 || n3 === void 0) continue;
        let r3 = s3.declarations.find((t6) => t6.kind === `global` && t6.name === e2.name);
        r3 && u3.set(r3.id, { group: t5, binding: n3 });
      }
      for (let e2 of o2.entries) {
        let t5 = s3.functions.find((t6) => t6.name === e2.name);
        if (c2 || !t5) {
          r2.set(e2, n2);
          continue;
        }
        let i3 = [t5.id], a3 = /* @__PURE__ */ new Set(), o3 = /* @__PURE__ */ new Map();
        for (; i3.length; ) {
          let e3 = i3.pop();
          if (!a3.has(e3) && (a3.add(e3), s3.functions[e3])) for (let t6 of s3.references) {
            if (t6.functionId !== e3) continue;
            let n3 = u3.get(t6.declarationId);
            n3 && o3.set(`${n3.group}:${n3.binding}`, n3);
            let r3 = l2.get(t6.declarationId);
            r3 !== void 0 && i3.push(r3);
          }
        }
        r2.set(e2, [...o3.values()].sort((e3, t6) => e3.group - t6.group || e3.binding - t6.binding));
      }
    }
    return r2;
  }
  function or(e, t4, n2, r2, i2, a2) {
    return { name: e.name, mangledName: e.mangledName, stage: e.stage, ...e.workgroupSize ? { workgroupSize: e.workgroupSize } : {}, bindings: i2.map(({ group: e2, binding: t5 }) => ({ group: e2, binding: t5 })), samplingPairs: a2, ...e.stage === `vertex` ? { inputs: sr(e, t4, n2, r2) } : {} };
  }
  function sr(e, t4, n2, r2) {
    let i2 = [];
    for (let a2 of e.params) {
      if (cr(a2.attrs, `builtin`)) continue;
      let o2 = z2(a2.type, e.path, n2, r2), s3 = F2(a2.attrs, `location`);
      if (s3 !== void 0) {
        i2.push({ name: a2.name, location: s3, type: o2 });
        continue;
      }
      let c2 = wn(o2, r2);
      if (c2.kind !== `identifier`) continue;
      let l2 = t4.find((e2) => e2.mangledName === (c2.mangledName ?? c2.name)), u3 = r2.structs.get(c2.mangledName ?? c2.name);
      if (l2) for (let e2 = 0; e2 < l2.members.length; e2++) {
        let t5 = l2.members[e2];
        if (cr(t5.attrs, `builtin`)) continue;
        let a3 = F2(t5.attrs, `location`);
        a3 !== void 0 && i2.push({ name: t5.name, location: a3, type: u3?.members[e2]?.type ?? z2(t5.type, l2.path, n2, r2) });
      }
    }
    return i2;
  }
  function cr(e, t4) {
    return e.some((e2) => e2.name === t4);
  }
  function lr(e, t4 = `<runtime>`) {
    let n2 = yt2(e, t4), r2 = et2(n2);
    if (r2.imports.length > 0) throw j2(`VGPU-WGSL-REFLECT-SOURCE-IMPORT`, `reflectSource() accepts a single raw WGSL string; use resolveShader() for WGSL import graphs.`);
    return ir([{ path: t4, source: e, tokens: n2, parsed: r2 }]);
  }
  function ur() {
    let e = /* @__PURE__ */ new Map();
    return { getOrCreate(t4, n2, r2, i2) {
      let a2 = r2.map(dr), o2 = `${t4}:${n2}:${a2.join(`|`)}`, s3 = e.get(o2);
      if (s3) return s3.bindGroup;
      let c2 = i2();
      return e.set(o2, { identities: a2, bindGroup: c2 }), c2;
    }, evictIdentity(t4) {
      let n2 = dr(t4);
      for (let [t5, r2] of e) r2.identities.includes(n2) && e.delete(t5);
    }, clearDraw(t4) {
      let n2 = `${t4}:`;
      for (let t5 of e.keys()) t5.startsWith(n2) && e.delete(t5);
    }, dispose() {
      e.clear();
    } };
  }
  function dr(e) {
    return typeof e == `string` || typeof e == `number` ? String(e) : `${e.kind}:${e.id}`;
  }
  function fr(e, t4, n2) {
    let r2 = e[t4];
    if (!r2) throw new Y({ code: `VGPU-REFLECT-ENTRY-METADATA-MISSING`, message: `Entry point '${e.name}' has no reflected ${t4}.`, fix: `Pass the reflection from reflectSource()/resolveShader().`, where: n2 });
    return r2;
  }
  var pr = /* @__PURE__ */ new WeakMap();
  function mr(e, t4) {
    if (!e.gpu.pushErrorScope || !e.gpu.popErrorScope) return;
    e.gpu.pushErrorScope(`validation`);
    let n2 = pr.get(e.gpu);
    n2 ? n2.push(t4) : pr.set(e.gpu, [t4]);
  }
  function H2(e) {
    let t4 = pr.get(e.gpu);
    if (!t4?.length || !e.gpu.popErrorScope) return;
    let n2 = t4.pop();
    return t4.length || pr.delete(e.gpu), { context: n2, error: e.gpu.popErrorScope() };
  }
  function hr(e) {
    let t4 = [], n2 = H2(e);
    for (; n2; ) t4.push(n2), n2 = H2(e);
    return t4;
  }
  function gr(e) {
    let t4 = H2(e);
    t4 && Cr(t4);
  }
  function _r2(e) {
    for (let t4 of hr(e)) Cr(t4);
  }
  function U2(e) {
    for (let t4 of e) Cr(t4);
  }
  function vr(e) {
    return e.gpu.queue.onSubmittedWorkDone?.() ?? Promise.resolve();
  }
  function yr(e, t4 = [], n2 = {}) {
    return Sr(e, t4, n2.errorSink ?? wr);
  }
  function br(e, t4) {
    return { context: e.context, error: xr(e.error, t4.error) };
  }
  async function xr(e, t4) {
    let n2 = await Promise.allSettled([e, t4]);
    for (let e2 of n2) if (e2.status === `fulfilled` && e2.value) return e2.value;
    let r2 = n2.find((e2) => e2.status === `rejected`);
    if (r2?.status === `rejected`) throw r2.reason;
    return null;
  }
  async function Sr(e, t4, n2) {
    await vr(e);
    for (let e2 of t4) try {
      let t5 = await e2.error;
      t5 && await n2(_e(e2.context.label, e2.context.group, t5));
    } catch (t5) {
      await n2(_e(e2.context.label, e2.context.group, t5));
    }
  }
  function Cr(e) {
    e.error.catch(() => void 0);
  }
  function wr(e) {
    console.error(e);
  }
  function Tr(e, t4, n2, r2) {
    try {
      t4.end();
    } catch (t5) {
      let i2 = hr(e);
      U2(n2), U2(i2), n2.length = 0;
      let a2 = i2[0]?.context ?? r2;
      throw a2 ? _e(a2.label, a2.group, t5) : t5;
    }
  }
  var Er = 1;
  var Dr = /* @__PURE__ */ new WeakMap();
  function Or(e) {
    return typeof e != `object` || !e || ArrayBuffer.isView(e) || e instanceof ArrayBuffer || Array.isArray(e) ? true : e instanceof _ || e instanceof U ? false : !Gr(e);
  }
  function kr(e) {
    return typeof e != `object` || !e || Array.isArray(e) || ArrayBuffer.isView(e) || e instanceof ArrayBuffer || e instanceof _ || e instanceof U ? false : !Gr(e);
  }
  function Ar(e, t4, n2) {
    try {
      return jr(e, t4, n2);
    } catch (r2) {
      if (r2?.code === `VGPU-CORE-TEXTURE-DESTROYED`) {
        let r3 = t4 instanceof U ? t4.label : Wr(t4)?.color.label;
        throw ue(n2.sourceHint, e, r3);
      }
      throw r2;
    }
  }
  function jr(e, t4, n2) {
    switch (e.bindingLayout?.kind) {
      case `buffer`:
        return Mr(e, t4, n2);
      case `texture`:
        return Nr(e, t4, n2);
      case `sampler`:
        return Rr(e, t4);
      case `storageTexture`:
        return Pr(e, t4);
      case `externalTexture`:
        throw dt(e, `external texture`, `Pass a compatible GPUExternalTexture.`);
      default:
        throw dt(e, `reflected resource`, `Fix shader reflection bindingLayout.`);
    }
  }
  function Mr(e, t4, n2) {
    let r2 = Je2(t4);
    if (r2) return r2[qe2](e, n2.sourceHint);
    if (t4 instanceof _) return Ke2(t4, `${n2.sourceHint}.set`), Br(e, t4.options.usage), { resource: { buffer: t4.gpu }, identity: t4.resourceIdentity, unsubscribe: (e2) => t4.onDestroy(e2) };
    if (Kr(t4)) return Ke2(t4.buffer, `${n2.sourceHint}.set`), { resource: { buffer: t4.gpu, offset: 0, size: t4.size }, identity: t4.buffer.resourceIdentity, unsubscribe: (e2) => t4.buffer.onDestroy(e2) };
    if (Jr(t4)) return { resource: t4, identity: W2(t4.buffer) };
    if (Yr(t4)) return { resource: { buffer: t4 }, identity: W2(t4) };
    throw dt(e, `buffer`, `Pass a compatible Buffer/Uniform: ${e.name}.set({ ${e.name}: gpu.device.createBuffer(...) }).`);
  }
  function Nr(e, t4, n2) {
    let r2 = e.bindingLayout?.kind === `texture` && e.bindingLayout.texture.sampleType === `depth`, i2 = Wr(t4);
    if (i2) {
      if (r2 && !i2.depth) throw dt(e, `a target with a depth attachment`, `Create it with target(gpu, { size, depth: true }) or bind a Texture: set({ ${e.name}: scene.depth }).`);
      let t5 = r2 ? i2.depth : i2.color;
      Hr(e, t5, n2);
      let a2 = i2.onTexturesRecreated?.bind(i2);
      return { resourceLabel: t5.label, resource: t5.createView(Ur(t5)), identity: t5.resourceIdentity, unsubscribe: (e2) => {
        let n3 = i2.onDestroy(e2), r3 = t5.onDestroy(e2);
        return () => {
          n3(), r3();
        };
      }, onRecreate: a2 ? (e2) => a2(e2) : void 0 };
    }
    if (t4 instanceof U) return Vr(e, t4.usage), Hr(e, t4, n2), { resourceLabel: t4.label, resource: t4.createView(Ur(t4)), identity: t4.resourceIdentity, unsubscribe: (e2) => t4.onDestroy(e2) };
    if (qr(t4)) return { resource: t4.createView(), identity: t4.resourceIdentity ?? W2(t4) };
    if (typeof t4 == `object` && t4) return { resource: t4, identity: W2(t4) };
    throw dt(e, `texture/target`, `Pass a Texture or Target: ${e.name}.set({ ${e.name}: scene.color }) or set({ ${e.name}: scene }).`);
  }
  function Pr(e, t4) {
    let n2 = e.bindingLayout?.kind === `storageTexture` ? e.bindingLayout.storageTexture : void 0, r2 = { format: n2?.format, viewDimension: n2?.viewDimension ?? `2d` }, i2 = `texture(gpu, { kind: "${r2.viewDimension}", size${r2.viewDimension === `2d-array` ? `, layers` : ``}, format: "${r2.format ?? `rgba8unorm`}", usage: ["storage_binding"] })`;
    if (Wr(t4)) throw dt(e, `a storage texture, not a Target`, `Render targets are not storage textures. Create one with ${i2} and set({ ${e.name}: texture }).`);
    if (t4 instanceof U) return Fr(e, t4, r2, i2), { resourceLabel: t4.label, resource: t4.createView(Ir(r2.viewDimension)), identity: t4.resourceIdentity, unsubscribe: (e2) => t4.onDestroy(e2) };
    if (qr(t4)) return { resource: t4.createView(Ir(r2.viewDimension)), identity: t4.resourceIdentity ?? W2(t4) };
    if (typeof t4 == `object` && t4) return { resource: t4, identity: W2(t4) };
    throw dt(e, `a storage texture`, `Pass a Texture from ${i2}: set({ ${e.name}: texture }).`);
  }
  function Fr(e, t4, n2, r2) {
    let i2 = t4.label ?? `texture`;
    if (!t4.usage.includes(`storage_binding`)) throw dt(e, `a texture with storage_binding usage`, `Create it with ${r2}; include texture_binding too if you also sample it.`);
    if (n2.format && t4.format !== n2.format) throw dt(e, `format ${n2.format}`, `Texture '${i2}' is ${t4.format}. Create it with ${r2} or declare texture_storage_${n2.viewDimension.replace(`-`, `_`)}<${t4.format}, ...> in WGSL.`);
    let a2 = Lr(n2.viewDimension);
    if (a2 && t4.dimension !== a2) throw dt(e, `dimension "${a2}"`, `Texture '${i2}' has kind "${t4.kind}". Create it with ${r2}.`);
  }
  function Ir(e) {
    return { dimension: e, baseMipLevel: 0, mipLevelCount: 1 };
  }
  function Lr(e) {
    switch (e) {
      case `1d`:
        return `1d`;
      case `2d`:
      case `2d-array`:
        return `2d`;
      case `3d`:
        return `3d`;
      default:
        return;
    }
  }
  function Rr(e, t4) {
    if (zr(t4)) return { resource: t4, identity: W2(t4) };
    throw dt(e, `sampler`, `Use the cached sampler: set({ ${e.name}: sampler(gpu) }).`);
  }
  function zr(e) {
    return typeof e != `object` || !e || e instanceof _ || e instanceof U ? false : !Yr(e) && !Jr(e) && !qr(e) && !Wr(e);
  }
  function Br(e, t4) {
    let n2 = e.bindingLayout?.kind === `buffer` ? e.bindingLayout.buffer.type : void 0;
    if (n2 === `uniform` && !t4.includes(`uniform`)) throw dt(e, `uniform buffer`, `Create with usage: ['uniform','copy_dst'].`);
    if ((n2 === `storage` || n2 === `read-only-storage`) && !t4.includes(`storage`)) throw dt(e, `storage buffer`, `Create with usage: ['storage','copy_dst'].`);
  }
  function Vr(e, t4) {
    if (!t4.includes(`texture_binding`) && !t4.includes(`render_attachment`)) throw dt(e, `sampled texture`, `Use texture_binding usage or a sampleable Target.`);
  }
  function Hr(e, t4, n2) {
    if (!(!n2.filterableTexture || n2.float32Filterable) && (t4.format === `r32float` || t4.format === `rg32float` || t4.format === `rgba32float`)) throw fe(n2.sourceHint, e, t4.format, t4.label ?? `texture`, n2.pairedSampler);
  }
  function Ur(e) {
    return e.format.includes(`stencil`) ? { aspect: `depth-only` } : void 0;
  }
  function Wr(e) {
    if (typeof e != `object` || !e) return;
    let t4 = e;
    if (!(!t4.resourceIdentity || !t4.color || typeof t4.onDestroy != `function`)) return t4;
  }
  function Gr(e) {
    let t4 = e;
    return `gpu` in t4 || `bindGroup` in t4 || `createView` in t4 || `resourceIdentity` in t4;
  }
  function W2(e) {
    if (typeof e != `object` || !e) return `value:${String(e)}`;
    let t4 = Dr.get(e);
    return t4 || (t4 = { kind: `external`, id: Er++ }, Dr.set(e, t4)), t4;
  }
  function Kr(e) {
    return typeof e == `object` && !!e && `gpu` in e && `size` in e && `buffer` in e && e.buffer instanceof _;
  }
  function qr(e) {
    return typeof e == `object` && !!e && typeof e.createView == `function`;
  }
  function Jr(e) {
    return typeof e == `object` && !!e && `buffer` in e && Yr(e.buffer);
  }
  function Yr(e) {
    return typeof e == `object` && !!e && `size` in e && `usage` in e && typeof e.destroy == `function`;
  }
  function Xr(e, t4) {
    Zr(e), oi(e, t4, `$`, 0, e.size);
    let n2 = new ArrayBuffer(e.size);
    return Qr(new DataView(n2), e, 0, t4), n2;
  }
  function Zr(e) {
    if (e.size === void 0) throw X(`set`, `No se puede inferir byteLength para layout runtime-sized '${e.name}'.`);
  }
  function Qr(e, t4, n2, r2) {
    if (t4.members) return $r(e, t4.members, n2, r2);
    ei(e, t4, n2, r2);
  }
  function $r(e, t4, n2, r2) {
    let i2 = r2;
    for (let r3 of t4) Qr(e, r3.layout, n2 + r3.offset, i2?.[r3.name]);
  }
  function ei(e, t4, n2, r2) {
    switch (t4.type.kind) {
      case `scalar`:
        return ti(e, n2, t4.type.name, r2);
      case `atomic`:
        return ni(e, n2, t4.type, r2);
      case `vector`:
        return ri(e, n2, t4.type, r2);
      case `matrix`:
        return ii(e, t4, n2, r2);
      case `array`:
        return ai(e, t4, n2, r2);
      default:
        throw X(`set`, `No hay writer para layout ${t4.type.kind}.`);
    }
  }
  function ti(e, t4, n2, r2) {
    n2 === `f32` ? e.setFloat32(t4, r2, true) : n2 === `i32` ? e.setInt32(t4, r2, true) : n2 === `u32` || n2 === `bool` ? e.setUint32(t4, n2 === `bool` ? +!!r2 : r2, true) : e.setUint16(t4, yi(r2), true);
  }
  function ni(e, t4, n2, r2) {
    ti(e, t4, K2(n2.element), r2);
  }
  function ri(e, t4, n2, r2) {
    let i2 = r2, a2 = vi(n2.element);
    for (let r3 = 0; r3 < n2.width; r3++) ti(e, t4 + r3 * a2, K2(n2.element), i2?.[r3] ?? 0);
  }
  function ii(e, t4, n2, r2) {
    let i2 = t4.type, a2 = r2, o2 = vi(i2.element), s3 = t4.stride ?? 16;
    for (let t5 = 0; t5 < i2.columns; t5++) for (let r3 = 0; r3 < i2.rows; r3++) ti(e, n2 + t5 * s3 + r3 * o2, K2(i2.element), a2?.[t5 * i2.rows + r3] ?? 0);
  }
  function ai(e, t4, n2, r2) {
    let i2 = r2, a2 = t4.stride ?? t4.element?.size ?? 0;
    if (!t4.element) throw X(`set`, `Array layout sin element layout.`);
    for (let r3 = 0; r3 < i2.length; r3++) Qr(e, t4.element, n2 + r3 * a2, i2[r3]);
  }
  function oi(e, t4, n2, r2, i2) {
    if (e.members) {
      si(e.members, t4, n2, r2, i2);
      return;
    }
    switch (e.type.kind) {
      case `scalar`:
        di(e.type.name, t4, n2), mi(n2, r2, vi(e.type), i2);
        return;
      case `atomic`:
        di(K2(e.type.element), t4, n2), mi(n2, r2, 4, i2);
        return;
      case `vector`:
        ci(e.type, t4, n2, r2, i2);
        return;
      case `matrix`:
        li(e, t4, n2, r2, i2);
        return;
      case `array`:
        ui(e, t4, n2, r2, i2);
        return;
      default:
        throw X(`set`, `No hay validator para layout ${e.type.kind}.`);
    }
  }
  function si(e, t4, n2, r2, i2) {
    gi(t4) || G2(`type`, n2, `an object`, _i2(t4));
    let a2 = new Set(e.map((e2) => e2.name)), o2 = Object.keys(t4).find((e2) => !a2.has(e2));
    o2 !== void 0 && G2(`unknown-field`, `${n2}.${o2}`, `a declared field`, o2);
    for (let a3 of e) {
      let e2 = `${n2}.${a3.name}`;
      Object.hasOwn(t4, a3.name) || G2(`missing-field`, e2, `a required field`, `missing`), oi(a3.layout, t4[a3.name], e2, r2 + a3.offset, i2);
    }
  }
  function ci(e, t4, n2, r2, i2) {
    let a2 = fi(t4, n2, e.width), o2 = K2(e.element), s3 = vi(e.element);
    for (let t5 = 0; t5 < e.width; t5++) di(o2, a2[t5], `${n2}[${t5}]`), mi(`${n2}[${t5}]`, r2 + t5 * s3, s3, i2);
  }
  function li(e, t4, n2, r2, i2) {
    let a2 = e.type, o2 = fi(t4, n2, a2.columns * a2.rows), s3 = K2(a2.element), c2 = vi(a2.element), l2 = e.stride;
    if (l2 === void 0) throw X(`set`, `Matrix layout '${e.name}' sin stride.`);
    for (let e2 = 0; e2 < a2.columns; e2++) for (let t5 = 0; t5 < a2.rows; t5++) {
      let u3 = e2 * a2.rows + t5, d2 = `${n2}[${e2}][${t5}]`;
      di(s3, o2[u3], d2), mi(d2, r2 + e2 * l2 + t5 * c2, c2, i2);
    }
  }
  function ui(e, t4, n2, r2, i2) {
    let a2 = pi(t4, n2), o2 = e.type;
    o2.count !== void 0 && a2.length !== o2.count && G2(`shape`, n2, o2.count, a2.length);
    let s3 = e.stride;
    if (s3 === void 0 || !e.element) throw X(`set`, `Array layout '${e.name}' sin stride o element layout.`);
    if (e.runtimeSized) {
      let t5 = hi(s3, a2.length, n2), o3 = e.size ?? i2 - r2;
      o3 !== t5 && G2(`extent`, n2, t5, o3);
    }
    for (let t5 = 0; t5 < a2.length; t5++) oi(e.element, a2[t5], `${n2}[${t5}]`, r2 + t5 * s3, i2);
  }
  function di(e, t4, n2) {
    if (e === `bool`) {
      typeof t4 != `boolean` && G2(`type`, n2, `bool`, _i2(t4), e);
      return;
    }
    typeof t4 != `number` && G2(`type`, n2, e, _i2(t4), e), e === `i32` && (!Number.isInteger(t4) || t4 < -2147483648 || t4 > 2147483647) && G2(`integer-range`, n2, `an integer in [-2147483648, 2147483647]`, String(t4), e), e === `u32` && (!Number.isInteger(t4) || t4 < 0 || t4 > 4294967295) && G2(`integer-range`, n2, `an integer in [0, 4294967295]`, String(t4), e);
  }
  function fi(e, t4, n2) {
    let r2 = pi(e, t4);
    return r2.length !== n2 && G2(`shape`, t4, n2, r2.length), r2;
  }
  function pi(e, t4) {
    if (Array.isArray(e) || ArrayBuffer.isView(e) && !(e instanceof DataView) && `length` in e) return e;
    G2(`type`, t4, `an array or typed array`, _i2(e));
  }
  function mi(e, t4, n2, r2) {
    (!Number.isSafeInteger(t4) || !Number.isSafeInteger(n2) || t4 < 0 || n2 < 0 || t4 > r2 - n2) && G2(`buffer-range`, e, `${n2} bytes inside ${r2}`, t4);
  }
  function hi(e, t4, n2) {
    let r2 = e * t4;
    return (!Number.isSafeInteger(e) || e < 0 || !Number.isSafeInteger(r2)) && G2(`extent`, n2, `a safe byte extent`, String(r2)), r2;
  }
  function G2(e, t4, n2, r2, i2) {
    let a2 = e === `shape` ? `expected exactly ${n2} values, received ${r2}` : e === `missing-field` ? `required field is missing` : e === `unknown-field` ? `field '${r2}' is not declared by the layout` : e === `integer-range` ? `${r2} is outside the exact ${i2} range` : e === `extent` ? `runtime extent requires ${n2} bytes, received ${r2}` : e === `buffer-range` ? `write at byte ${r2} exceeds ${n2}` : `expected ${n2}, received ${r2}`;
    throw ft(i2 === void 0 ? { reason: e, path: t4, expected: n2, actual: r2 } : { reason: e, path: t4, expected: n2, actual: r2, type: i2 }, a2);
  }
  function gi(e) {
    if (typeof e != `object` || !e || Array.isArray(e) || ArrayBuffer.isView(e)) return false;
    let t4 = Object.getPrototypeOf(e);
    return t4 === Object.prototype || t4 === null;
  }
  function _i2(e) {
    return e === null ? `null` : Array.isArray(e) ? `array` : ArrayBuffer.isView(e) ? e.constructor.name : typeof e;
  }
  function vi(e) {
    return K2(e) === `f16` ? 2 : 4;
  }
  function K2(e) {
    if (e.kind !== `scalar`) throw X(`set`, `Expected scalar, got ${e.kind}`);
    return e.name;
  }
  function yi(e) {
    let t4 = new Float32Array(1), n2 = new Uint32Array(t4.buffer);
    t4[0] = e;
    let r2 = n2[0], i2 = r2 >> 16 & 32768, a2 = r2 & 8388607, o2 = r2 >> 23 & 255;
    if (o2 === 255) return i2 | (a2 ? 32256 : 31744);
    let s3 = o2 - 127 + 15;
    if (s3 >= 31) return i2 | 31744;
    if (s3 <= 0) return s3 < -10 ? i2 : i2 | bi(a2 | 8388608, 14 - s3);
    let c2 = bi(a2, 13);
    if (c2 === 1024) {
      let e2 = s3 + 1;
      return e2 >= 31 ? i2 | 31744 : i2 | e2 << 10;
    }
    return i2 | s3 << 10 | c2;
  }
  function bi(e, t4) {
    let n2 = e >>> t4, r2 = e & (1 << t4) - 1, i2 = 1 << t4 - 1;
    return r2 > i2 || r2 === i2 && (n2 & 1) == 1 ? n2 + 1 : n2;
  }
  var xi = /* @__PURE__ */ new WeakMap();
  function Si(e, t4) {
    let n2 = /* @__PURE__ */ new Map(), r2 = /* @__PURE__ */ new Set(), i2 = /* @__PURE__ */ new Set(), a2 = new Set(e.filter((e2) => e2.bindingLayout?.kind === `texture` && e2.bindingLayout.texture.sampleType === `depth`).map((e2) => `${e2.group}:${e2.binding}`));
    for (let e2 of t4) {
      let t5 = e2.stage === `vertex` ? 1 : e2.stage === `fragment` ? 2 : 4;
      for (let r3 of fr(e2, `bindings`, `visibility`)) {
        let e3 = `${r3.group}:${r3.binding}`;
        n2.set(e3, (n2.get(e3) ?? 0) | t5);
      }
      for (let t6 of fr(e2, `samplingPairs`, `visibility`)) {
        if (t6.mode !== `filtering`) continue;
        let e3 = `${t6.texture.group}:${t6.texture.binding}`;
        a2.has(e3) ? i2.add(`${t6.sampler.group}:${t6.sampler.binding}`) : r2.add(e3);
      }
    }
    let o2 = (e2) => n2.get(`${e2.group}:${e2.binding}`) ?? 0;
    return Object.defineProperty(o2, "filterable", { value: r2 }), Object.defineProperty(o2, "nonFilteringSamplers", { value: i2 }), o2;
  }
  function Ci(e, t4, n2 = ki) {
    return e.flatMap((e2) => {
      if (e2.group !== t4) return [];
      let r2 = n2(e2), i2 = `${e2.group}:${e2.binding}`;
      return r2 === 0 ? [] : [{ binding: e2.binding, visibility: r2, ...Di(e2, n2.filterable?.has(i2) ?? false, n2.nonFilteringSamplers?.has(i2) ?? false) }];
    });
  }
  function wi(e, t4, n2, r2 = ki) {
    let i2 = /* @__PURE__ */ new Map(), a2 = n2.bindings.filter((e2) => r2(e2) !== 0).map((e2) => e2.group), o2 = Math.max(-1, ...a2);
    for (let a3 = 0; a3 <= o2; a3++) i2.set(a3, Ti(e, t4, n2, a3, r2));
    return i2;
  }
  function Ti(e, t4, n2, r2, i2 = ki) {
    return Ei(e, `${t4}.group${r2}.bgl`, Ci(n2.bindings, r2, i2));
  }
  function Ei(e, t4, n2) {
    let r2 = xi.get(e.gpu);
    r2 || (r2 = /* @__PURE__ */ new Map(), xi.set(e.gpu, r2));
    let i2 = JSON.stringify(n2), a2 = r2.get(i2);
    if (a2) return a2;
    let o2 = He2(e.gpu.createBindGroupLayout({ label: t4, entries: n2 }), { entries: n2 });
    return r2.set(i2, o2), o2;
  }
  function Di(e, t4, n2) {
    let r2 = e.bindingLayout;
    if (!r2) throw X(`bindGroupLayout`, `Binding '${e.name}' does not have a reflected bindingLayout.`);
    return t4 && r2.kind === `texture` && r2.texture.sampleType === `unfilterable-float` && !r2.texture.multisampled ? { texture: { ...r2.texture, sampleType: `float` } } : n2 && r2.kind === `sampler` && r2.sampler.type === `filtering` ? { sampler: { type: `non-filtering` } } : Oi(r2);
  }
  function Oi(e) {
    switch (e.kind) {
      case `buffer`:
        return { buffer: { ...e.buffer } };
      case `sampler`:
        return { sampler: { ...e.sampler } };
      case `texture`:
        return { texture: { ...e.texture } };
      case `storageTexture`:
        return { storageTexture: { ...e.storageTexture } };
      case `externalTexture`:
        return { externalTexture: {} };
    }
  }
  function ki(e) {
    let t4 = globalThis.GPUShaderStage, n2 = t4?.VERTEX ?? 1, r2 = t4?.FRAGMENT ?? 2, i2 = t4?.COMPUTE ?? 4;
    return e.kind === `buffer` ? n2 | r2 | i2 : r2 | i2;
  }
  function Ai(e) {
    let t4 = ji(e.reflection), n2 = [...e.bindGroupLayouts.keys()].sort((e2, t5) => e2 - t5), r2 = /* @__PURE__ */ new Map();
    function i2(e2) {
      let t5 = [];
      for (let [n3, r3] of Object.entries(e2)) t5.push(...o2(n3, r3));
      return t5;
    }
    function a2(t5) {
      let n3 = e.bindGroupLayouts.get(t5.info.group);
      return !!n3 && !!Ue2(n3)?.entries.some((e2) => e2.binding === t5.info.binding);
    }
    function o2(n3, r3) {
      let i3 = t4.get(n3);
      if (i3) return s3(i3, n3, r3);
      let a3 = Mi(n3, t4, e.label);
      if (!a3) throw X(`${e.label}.set`, `Binding '${n3}' does not exist in '${e.label}'.`);
      return c2(a3, n3, r3);
    }
    function s3(e2, t5, n3) {
      C2(e2.info.group);
      let r3 = Ni(e2.info, n3);
      Pi(e2, t5, r3);
      let i3 = Vi(e2.identity);
      return r3 === `lib` ? l2(e2, Hi(e2.libValue, n3)) : d2(e2, n3), e2.ownership ?? (e2.ownership = r3), a2(e2) ? Bi(e2, i3) : [];
    }
    function c2(t5, n3, r3) {
      C2(t5.info.group);
      let i3 = Ni(t5.info, r3);
      if (Pi(t5, n3, i3), Fi(t5, n3, i3), i3 !== `lib`) throw X(`${e.label}.set`, `Member '${n3}' needs a JS value; set resource '${t5.info.name}' instead.`);
      let o3 = Vi(t5.identity);
      return l2(t5, { ...Ui(t5.libValue ?? Wi(w2(t5))), [n3]: r3 }), t5.ownership ?? (t5.ownership = i3), t5.memberOwnership.set(n3, i3), a2(t5) ? Bi(t5, o3) : [];
    }
    function l2(e2, t5) {
      let n3 = w2(e2), r3 = Xr(n3, t5);
      e2.libValue = t5, e2.buffer || te2(e2, n3.size), e2.bytes = r3, e2.buffer.write(r3, 0);
    }
    function u3(t5) {
      let n3 = Ue2(e.bindGroupLayouts.get(t5.group))?.entries.find((e2) => e2.binding === t5.binding), r3 = e.reflection.entryPoints.flatMap((t6) => fr(t6, `samplingPairs`, e.label)).find((e2) => e2.mode === `filtering` && e2.texture.group === t5.group && e2.texture.binding === t5.binding), i3 = r3 && e.reflection.bindings.find((e2) => e2.group === r3.sampler.group && e2.binding === r3.sampler.binding);
      return { sourceHint: e.label, filterableTexture: n3?.texture?.sampleType === `float`, float32Filterable: e.device.features.has(`float32-filterable`), pairedSampler: i3 };
    }
    function d2(e2, t5) {
      let n3 = Ar(e2.info, t5, u3(e2.info));
      e2.unsubscribe?.(), e2.unsubscribeRecreate?.(), e2.resource = n3.resource, e2.identity = n3.identity, e2.destroyed = false, e2.resourceLabel = n3.resourceLabel, e2.subscribeDestroy = n3.unsubscribe, e2.unsubscribe = n3.unsubscribe?.(() => m2(e2)), e2.unsubscribeRecreate = n3.onRecreate?.(() => f2(e2, t5));
    }
    function f2(t5, n3) {
      let r3 = Vi(t5.identity);
      if (t5.identity && e.cache.evictIdentity(t5.identity), d2(t5, n3), a2(t5)) for (let n4 of Bi(t5, r3)) e.onIdentityChange?.(n4);
    }
    function p2(t5, n3, i3) {
      _2(t5), Ii(e.label, t5, n3, i3);
      let a3 = r2.has(t5) ? `claimed-group:${t5}` : void 0;
      return r2.set(t5, n3), a3;
    }
    function m2(t5) {
      t5.destroyed || (t5.destroyed = true, t5.identity && e.cache.evictIdentity(t5.identity), a2(t5) && !r2.has(t5.info.group) && e.onIdentityChange?.({ group: t5.info.group, binding: t5.info.binding, bindingName: t5.info.name, bindingKind: t5.info.kind, previousIdentity: Vi(t5.identity), newIdentity: `destroyed:${Vi(t5.identity)}` }));
    }
    function h2() {
      for (let n3 of t4.values()) if (n3.destroyed && a2(n3) && !r2.has(n3.info.group)) throw ue(e.label, n3.info, n3.resourceLabel);
    }
    function g2(e2) {
      h2();
      let n3 = [];
      for (let i3 of t4.values()) {
        if (!a2(i3) || r2.has(i3.info.group) || !i3.subscribeDestroy) continue;
        let t5 = Vi(i3.identity), { group: o3, binding: s4, name: c3, kind: l3 } = i3.info;
        n3.push(i3.subscribeDestroy(() => e2({ group: o3, binding: s4, bindingName: c3, bindingKind: l3, previousIdentity: t5, newIdentity: `destroyed:${t5}` })));
      }
      return () => {
        for (let e3 of n3.splice(0)) e3();
      };
    }
    function _2(t5) {
      let n3 = e.bindGroupLayouts.get(t5);
      if (!n3) throw X(`${e.label}.layout`, `@group(${t5}) does not exist in '${e.label}'.`);
      return n3;
    }
    function v2() {
      return h2(), n2.map(y2);
    }
    function y2(t5) {
      let n3 = r2.get(t5);
      if (n3) return { group: t5, bindGroup: n3, offsets: [], claimValidation: b2(n3, t5) };
      let i3 = new Set(Ue2(_2(t5))?.entries.map((e2) => e2.binding)), a3 = e.reflection.bindings.filter((e2) => e2.group === t5 && i3.has(e2.binding)), o3 = x2(a3), s4 = ee2(a3);
      return { group: t5, bindGroup: e.cache.getOrCreate(e.drawId, t5, s4, () => e.device.gpu.createBindGroup({ label: `${e.label}.group${t5}`, layout: _2(t5), entries: o3 })), offsets: [] };
    }
    function b2(t5, n3) {
      return We2(t5) ? void 0 : { label: e.label, group: n3 };
    }
    function x2(e2) {
      return e2.map((e3) => {
        let t5 = S2(e3);
        return { binding: e3.binding, resource: t5.resource };
      });
    }
    function ee2(e2) {
      return e2.map((e3) => S2(e3).identity);
    }
    function S2(n3) {
      let r3 = t4.get(n3.name);
      if (!r3?.resource || !r3.identity) throw pe(e.label, n3);
      return r3;
    }
    function C2(t5) {
      if (r2.has(t5)) throw he(e.label, t5);
    }
    function te2(t5, n3) {
      t5.buffer = e.device.createBuffer({ size: n3, usage: [`uniform`, `copy_dst`], label: `${e.label}.${t5.info.name}` }), t5.resource = { buffer: t5.buffer.gpu, offset: 0, size: n3 }, t5.identity = t5.buffer.resourceIdentity, t5.unsubscribe = t5.buffer.onDestroy(() => e.cache.evictIdentity(t5.buffer.resourceIdentity));
    }
    function w2(t5) {
      if (t5.info.kind !== `buffer` || !t5.info.layout?.size) throw X(`${e.label}.set`, `Binding '${t5.info.name}' needs a compatible resource, not JS.`);
      return t5.info.layout;
    }
    return { assertUsable: h2, watchResources: g2, get groups() {
      return n2;
    }, set: i2, claimGroup: p2, layout: _2, bindGroups: v2, bindingState(e2) {
      let n3 = t4.get(e2);
      if (!(!n3?.ownership || !n3.resource || !n3.identity)) return { info: n3.info, ownership: n3.ownership, resource: n3.resource, identity: n3.identity };
    } };
  }
  function ji(e) {
    return new Map(e.bindings.map((e2) => [e2.name, { info: e2, memberOwnership: /* @__PURE__ */ new Map() }]));
  }
  function Mi(e, t4, n2) {
    let r2;
    for (let i2 of t4.values()) if (i2.info.layout?.members?.some((t5) => t5.name === e)) {
      if (r2) throw X(`${n2}.set`, `Binding member '${e}' is ambiguous in '${n2}'; set the complete binding.`);
      r2 = i2;
    }
    return r2;
  }
  function Ni(e, t4) {
    return e.bindingLayout?.kind === `buffer` && Or(t4) ? `lib` : `user`;
  }
  function Pi(e, t4, n2) {
    if (e.ownership && e.ownership !== n2) throw me(t4, e.ownership);
  }
  function Fi(e, t4, n2) {
    let r2 = e.memberOwnership.get(t4);
    if (r2 && r2 !== n2) throw me(t4, r2);
  }
  function Ii(e, t4, n2, r2) {
    let i2 = We2(n2);
    if (!i2) return;
    let a2 = Ue2(r2);
    if (!a2) return;
    let o2 = Li(a2.entries, i2.layout.entries);
    if (o2) throw ge(e, t4, o2);
  }
  function Li(e, t4) {
    if (e.length !== t4.length) return `expected ${e.length} bindings and received ${t4.length}`;
    let n2 = Ri(e), r2 = Ri(t4);
    for (let [e2, t5] of n2) {
      let n3 = r2.get(e2);
      if (!n3) return `missing @binding(${e2})`;
      if (zi(t5) !== zi(n3)) return `@binding(${e2}) does not match the reflected layout`;
    }
  }
  function Ri(e) {
    return new Map(e.map((e2) => [e2.binding, e2]));
  }
  function zi(e) {
    return JSON.stringify({ binding: e.binding, visibility: e.visibility, buffer: e.buffer, sampler: e.sampler, texture: e.texture, storageTexture: e.storageTexture, externalTexture: e.externalTexture ? {} : void 0 });
  }
  function Bi(e, t4) {
    let n2 = Vi(e.identity);
    return !n2 || t4 === n2 ? [] : [{ group: e.info.group, binding: e.info.binding, bindingName: e.info.name, bindingKind: e.info.kind, previousIdentity: t4, newIdentity: n2 }];
  }
  function Vi(e) {
    return e === void 0 ? void 0 : dr(e);
  }
  function Hi(e, t4) {
    return kr(e) && kr(t4) ? { ...e, ...t4 } : t4;
  }
  function Ui(e) {
    return kr(e) ? e : {};
  }
  function Wi(e) {
    if (e.members) return Object.fromEntries(e.members.map((e2) => [e2.name, Wi(e2.layout)]));
    switch (e.type.kind) {
      case `scalar`:
      case `atomic`:
        return 0;
      case `vector`:
        return Array.from({ length: e.type.width }, () => 0);
      case `matrix`:
        return Array.from({ length: e.type.columns * e.type.rows }, () => 0);
      case `array`: {
        let t4 = e.element;
        return e.type.count === void 0 || !t4 ? [] : Array.from({ length: e.type.count }, () => Wi(t4));
      }
      default:
        return;
    }
  }
  var Gi = Object.freeze([0, 0, 0, 1]);
  function Ki(e, t4) {
    let n2 = e, r2 = Array.isArray(e) ? e : [n2?.r, n2?.g, n2?.b, n2?.a];
    if (r2.length !== 4 || !r2.every((e2) => typeof e2 == `number` && Number.isFinite(e2))) throw at(t4);
    return qi(e);
  }
  function qi(e) {
    let t4 = e;
    return Array.isArray(e) ? [e[0], e[1], e[2], e[3]] : { r: t4.r, g: t4.g, b: t4.b, a: t4.a };
  }
  function Ji(e) {
    return e.colors ?? [{ format: e.format ?? `rgba8unorm` }];
  }
  function Yi(e) {
    return e.depth === true ? `depth24plus` : e.depth || void 0;
  }
  function Xi(e) {
    let t4 = e.msaa;
    if (t4 === true || t4 === 4) return 4;
    if (t4 === void 0 || t4 === false) return 1;
    let n2 = Qe();
    throw n2.code = `VGPU-TARGET-MSAA-INVALID`, n2.message = `msaa received ${t4}; WebGPU 1|4; use true`, n2;
  }
  function Zi(e, t4) {
    if (!Array.isArray(e?.size) || e.size.length !== 2 || !Array.from(e.size).every((e2) => Number.isSafeInteger(e2) && e2 > 0)) throw Qe();
    let n2 = Yi(e);
    if (n2 === `stencil8`) throw Ze(n2);
    if (Xi(e) === 4) for (let n3 of Ji(e)) Qi(n3.format, t4);
  }
  function Qi(e, t4) {
    if (t4.isCompatibilityMode && e === `rgba16float`) throw X(`target`, `Dawn compatibility mode does not support rgba16float+msaa.`, `Use rgba8unorm for MSAA here, or disable msaa.`);
  }
  function $i(e, t4, n2, r2) {
    let i2 = { view: (t4 ?? e).createView(), resolveTarget: t4 ? e.createView() : void 0, loadOp: r2 ? `load` : `clear`, storeOp: t4 ? `discard` : `store` };
    return r2 || (i2.clearValue = na(n2)), i2;
  }
  function ea(e, t4, n2, r2, i2) {
    if (i2) {
      let t5 = { view: e.createView(), depthReadOnly: true };
      return ta(e.format) && (t5.stencilReadOnly = true), t5;
    }
    let a2 = { view: e.createView(), depthLoadOp: t4 ? `load` : `clear`, depthStoreOp: e.sampleCount > 1 ? `discard` : `store` };
    return t4 || (a2.depthClearValue = n2 ?? 1), e.format && ta(e.format) && (a2.stencilLoadOp = t4 ? `load` : `clear`, a2.stencilStoreOp = e.sampleCount > 1 ? `discard` : `store`, t4 || (a2.stencilClearValue = r2 ?? 0)), a2;
  }
  function ta(e) {
    return !!e && e.includes(`stencil`);
  }
  function na(e) {
    return Array.isArray(e) ? { r: e[0], g: e[1], b: e[2], a: e[3] } : e;
  }
  function ra(e, t4) {
    return e[0] === t4[0] && e[1] === t4[1];
  }
  function ia(e) {
    return typeof e == `object` && !!e && typeof e.renderPassDescriptor == `function`;
  }
  var aa = 1;
  var oa = 1;
  var sa = /* @__PURE__ */ new WeakMap();
  var ca = /* @__PURE__ */ new WeakMap();
  function la(e) {
    return ia(e) ? { colors: e.colors.map((e2) => e2.format), depth: e.depth?.format, sampleCount: e.sampleCount } : typeof e != `object` || !e ? { colors: [] } : { colors: Array.isArray(e.colors) ? [...e.colors] : e.colors ?? [], depth: e.depth, sampleCount: e.sampleCount ?? 1 };
  }
  function ua(e) {
    return `${e.colors.join(`,`)}:${e.depth ?? `none`}:${e.sampleCount ?? 1}`;
  }
  function da(e, t4) {
    if (!Array.isArray(e.colors) || e.colors.length === 0) throw Xe(t4, `colors must be a non-empty array.`);
    let n2 = e.colors.find((e2) => typeof e2 != `string` || e2.length === 0);
    if (n2 !== void 0) throw Xe(t4, `colors must contain only GPUTextureFormat strings; received ${String(n2)}.`);
    if (e.depth !== void 0 && (typeof e.depth != `string` || e.depth.length === 0)) throw Xe(t4, `depth must be a GPUTextureFormat string.`);
    let r2 = e.sampleCount ?? 1;
    if (r2 !== 1 && r2 !== 4) throw Xe(t4, `sampleCount must be 1 or 4; received ${String(r2)}.`);
  }
  function fa(e) {
    let t4 = `${Ta(sa, e.module, () => aa++)}|${Ta(ca, e.pipelineLayout, () => oa++)}|${Ea(e.vertexBufferLayouts ?? [])}|${ua(e.signature)}`, n2 = e.topology || e.stripIndexFormat ? `${t4}|${e.topology ?? `triangle-list`}|${e.stripIndexFormat ?? `none`}` : t4, r2 = e.cullMode || e.frontFace ? `${n2}|${e.cullMode ?? `none`}|${e.frontFace ?? `ccw`}` : n2, i2 = e.unclippedDepth ? `${r2}|unclipped` : r2, a2 = e.depthKey ? `${i2}|${e.depthKey}` : i2, o2 = e.stencilKey ? `${a2}|${e.stencilKey}` : a2, s3 = e.multisampleKey ? `${o2}|${e.multisampleKey}` : o2, c2 = e.constantsKey ? `${s3}|${e.constantsKey}` : s3, l2 = e.entryKey ? `${c2}|${e.entryKey}` : c2;
    return e.fragmentKey ? `${l2}|${e.fragmentKey}` : l2;
  }
  function pa(e, t4, n2, r2, i2) {
    if (r2 === void 0) return t4.find((e2) => e2.stage === n2);
    if (typeof r2 != `string`) throw ke(e, `${n2} received ${ya(r2)}; expected an entry point name string.`, i2);
    let a2 = t4.find((e2) => e2.name === r2);
    if (!a2) throw ke(e, `"${r2}" matches no entry point in the shader; available entry points: ${ma(t4)}.`, i2);
    if (a2.stage !== n2) throw ke(e, `"${r2}" is a @${a2.stage} entry point, not @${n2}; available entry points: ${ma(t4)}.`, i2);
    return a2;
  }
  function ma(e) {
    return e.length ? e.map((e2) => `"${e2.name}" (@${e2.stage})`).join(`, `) : `none`;
  }
  function ha(e, t4, n2, r2) {
    if (t4 !== void 0 && (typeof t4 != `object` || !t4 || Array.isArray(t4))) throw Oe(e, `received ${ya(t4)}; expected { overrideNameOrId: number | boolean }.`, r2);
    let i2 = new Map(n2.map((e2) => [ga(e2), e2])), a2 = {};
    for (let [o2, s3] of Object.entries(t4 ?? {})) {
      if (!i2.has(o2)) throw Oe(e, `"${o2}" matches no override in the shader; available overrides: ${_a6(n2)}.`, r2);
      if (typeof s3 == `boolean`) {
        a2[o2] = +!!s3;
        continue;
      }
      if (typeof s3 != `number` || !Number.isFinite(s3)) throw Oe(e, `"${o2}" received ${ya(s3)}; use a finite number or a boolean (WebGPU converts the value to the override's WGSL type, and NaN/Infinity fail that conversion).`, r2);
      a2[o2] = s3;
    }
    for (let t5 of n2) {
      let n3 = ga(t5);
      if (t5.defaultValue === void 0 && !(n3 in a2)) throw Oe(e, `override '${t5.name}' has no default value and must be provided; add constants: { "${n3}": value }.`, r2);
    }
    return Object.keys(a2).length === 0 ? {} : { constants: a2, constantsKey: va(a2) };
  }
  function ga(e) {
    return e.id === void 0 ? e.name : String(e.id);
  }
  function _a6(e) {
    return e.length ? e.map((e2) => e2.id === void 0 ? `"${e2.name}"` : `"${e2.id}" (@id of ${e2.name})`).join(`, `) : `none`;
  }
  function va(e) {
    return `cn~${Object.entries(e).sort(([e2], [t4]) => e2 < t4 ? -1 : +(e2 > t4)).map(([e2, t4]) => `${e2}=${t4}`).join(`~`)}`;
  }
  function ya(e) {
    if (typeof e == `string`) return `"${e}"`;
    try {
      return JSON.stringify(e) ?? String(e);
    } catch {
      return String(e);
    }
  }
  function ba(e) {
    let t4 = /* @__PURE__ */ new Map();
    return { get(n2, r2) {
      let i2 = t4.get(n2);
      return i2 || (i2 = e.gpu.createShaderModule({ label: r2, code: n2 }), t4.set(n2, i2)), i2;
    }, dispose() {
      t4.clear();
    } };
  }
  function xa(e) {
    let t4 = /* @__PURE__ */ new Map();
    return { get(n2) {
      let r2 = Da(n2), i2 = t4.get(r2);
      return i2 || (i2 = e.gpu.createPipelineLayout({ bindGroupLayouts: Oa(n2) }), t4.set(r2, i2)), i2;
    }, dispose() {
      t4.clear();
    } };
  }
  function Sa(e, t4 = {}) {
    return new Ca(e, t4);
  }
  var _e3, _t4, _n3, _r3, _i3, _Ca_instances, a_fn, o_fn, s_fn, c_fn, l_fn, _a7;
  var Ca = (_a7 = class {
    constructor(e, t4) {
      __privateAdd(this, _Ca_instances);
      __publicField(this, "device");
      __privateAdd(this, _e3, /* @__PURE__ */ new Map());
      __privateAdd(this, _t4, /* @__PURE__ */ new Set());
      __privateAdd(this, _n3);
      __privateAdd(this, _r3);
      __privateAdd(this, _i3, false);
      this.device = e, __privateSet(this, _n3, t4.errorSink ?? (() => void 0)), __privateSet(this, _r3, t4.registerSettledSource?.(() => [...__privateGet(this, _t4)]));
    }
    getReady(e) {
      return __privateGet(this, _e3).get(e)?.pipeline;
    }
    getSync(e, t4, n2) {
      __privateMethod(this, _Ca_instances, c_fn).call(this, n2.where);
      let r2 = __privateGet(this, _e3).get(e);
      if (r2?.pipeline) return r2.pipeline;
      let i2 = r2 ?? {};
      r2 || __privateGet(this, _e3).set(e, i2);
      let a2 = __privateMethod(this, _Ca_instances, a_fn).call(this, e, i2, t4, n2);
      if (!a2) {
        i2.pending || __privateGet(this, _e3).delete(e);
        return;
      }
      return i2.pipeline = a2, i2.pending?.resolve(a2), i2.pending = void 0, a2;
    }
    getAsync(e, t4, n2) {
      __privateMethod(this, _Ca_instances, c_fn).call(this, n2.where);
      let r2 = __privateGet(this, _e3).get(e);
      if (r2?.pipeline) return Promise.resolve(r2.pipeline);
      if (r2?.pending) return r2.pending.promise;
      let i2 = {}, a2 = wa();
      i2.pending = a2, __privateGet(this, _e3).set(e, i2);
      let o2;
      try {
        o2 = t4();
      } catch (t5) {
        let r3 = Je(n2.where, t5, n2.signature);
        return a2.reject(r3), __privateGet(this, _e3).delete(e), a2.promise;
      }
      return __privateMethod(this, _Ca_instances, l_fn).call(this, o2), o2.then((t5) => {
        __privateGet(this, _e3).get(e) !== i2 || i2.pipeline || i2.pending !== a2 || (i2.pipeline = t5, i2.pending = void 0, a2.resolve(t5));
      }, (t5) => {
        __privateGet(this, _e3).get(e) !== i2 || i2.pipeline || i2.pending !== a2 || (i2.pending = void 0, __privateGet(this, _e3).delete(e), a2.reject(Je(n2.where, t5, n2.signature)));
      }), a2.promise;
    }
    dispose() {
      var _a17;
      if (__privateGet(this, _i3)) return;
      __privateSet(this, _i3, true);
      let e = Ye(`gpu.dispose`);
      for (let t4 of __privateGet(this, _e3).values()) t4.pending?.reject(e);
      __privateGet(this, _e3).clear(), __privateGet(this, _t4).clear(), (_a17 = __privateGet(this, _r3)) == null ? void 0 : _a17.call(this);
    }
  }, _e3 = new WeakMap(), _t4 = new WeakMap(), _n3 = new WeakMap(), _r3 = new WeakMap(), _i3 = new WeakMap(), _Ca_instances = new WeakSet(), a_fn = function(e, t4, n2, r2) {
    let i2 = this.device.gpu, a2 = typeof i2.pushErrorScope == `function` && typeof i2.popErrorScope == `function`;
    a2 && i2.pushErrorScope(`validation`);
    try {
      let i3 = n2();
      return a2 && __privateMethod(this, _Ca_instances, o_fn).call(this, e, t4, r2), i3;
    } catch (e2) {
      a2 && __privateMethod(this, _Ca_instances, s_fn).call(this);
      let t5 = Je(r2.where, e2, r2.signature);
      __privateGet(this, _n3).call(this, t5);
      return;
    }
  }, o_fn = function(e, t4, n2) {
    let r2 = this.device.gpu.popErrorScope().then((r3) => {
      if (!r3) return;
      let i2 = Je(n2.where, r3, n2.signature);
      return __privateGet(this, _e3).get(e) === t4 && __privateGet(this, _e3).delete(e), __privateGet(this, _n3).call(this, i2);
    }, (r3) => {
      let i2 = Je(n2.where, r3, n2.signature);
      return __privateGet(this, _e3).get(e) === t4 && __privateGet(this, _e3).delete(e), __privateGet(this, _n3).call(this, i2);
    });
    __privateMethod(this, _Ca_instances, l_fn).call(this, r2);
  }, s_fn = function() {
    let e = this.device.gpu.popErrorScope?.();
    e && e.catch(() => void 0);
  }, c_fn = function(e) {
    if (__privateGet(this, _i3)) throw Ye(e);
  }, l_fn = function(e) {
    __privateGet(this, _t4).add(e), e.catch(() => void 0).then(() => __privateGet(this, _t4).delete(e), () => __privateGet(this, _t4).delete(e));
  }, _a7);
  function wa() {
    let e, t4, n2 = new Promise((n3, r2) => {
      e = n3, t4 = r2;
    });
    return n2.catch(() => void 0), { promise: n2, resolve: e, reject: t4 };
  }
  function Ta(e, t4, n2) {
    let r2 = e.get(t4);
    return r2 || (r2 = n2(), e.set(t4, r2)), r2;
  }
  function Ea(e) {
    return JSON.stringify(e.map((e2) => ({ arrayStride: e2.arrayStride, stepMode: e2.stepMode ?? `vertex`, attributes: [...e2.attributes].map((e3) => ({ shaderLocation: e3.shaderLocation, offset: e3.offset, format: e3.format })) })));
  }
  function Da(e) {
    return JSON.stringify([...e.entries()].map(([e2, t4]) => ({ group: e2, entries: Aa(t4) })));
  }
  function Oa(e) {
    let t4 = Math.max(-1, ...e.keys()), n2 = [];
    for (let r2 = 0; r2 <= t4; r2++) n2.push(ka(e, r2));
    return n2;
  }
  function ka(e, t4) {
    let n2 = e.get(t4);
    if (!n2) throw qe(t4);
    return n2;
  }
  function Aa(e) {
    return (Ue2(e)?.entries ?? []).map((e2) => ({ binding: e2.binding, visibility: e2.visibility, buffer: e2.buffer ? { ...e2.buffer } : void 0, sampler: e2.sampler ? { ...e2.sampler } : void 0, texture: e2.texture ? { ...e2.texture } : void 0, storageTexture: e2.storageTexture ? { ...e2.storageTexture } : void 0, externalTexture: e2.externalTexture ? { ...e2.externalTexture } : void 0 }));
  }
  var ja = gt(`frame-state`);
  function Ma(e) {
    return e.service(ja, Na);
  }
  function Na() {
    let e = /* @__PURE__ */ new Set(), t4 = Pa(), n2 = false, r2 = false, i2 = { time: 0, deltaTime: 0, frameCount: 0, advanceBy(e2) {
      i2.deltaTime = e2, i2.time += e2, r2 = true;
    }, tick() {
      if (n2) throw st();
      n2 = true;
      try {
        let n3 = Pa();
        r2 ? r2 = false : (i2.deltaTime = Math.max(0, (n3 - t4) / 1e3), i2.time += i2.deltaTime), t4 = n3, i2.frameCount += 1;
        for (let t5 of [...e]) t5();
      } finally {
        n2 = false;
      }
    }, onAdvance(t5) {
      return e.add(t5), () => {
        e.delete(t5);
      };
    } };
    return i2;
  }
  function Pa() {
    return globalThis.performance?.now?.() ?? Date.now();
  }
  function Fa(e, t4, n2 = {}) {
    let r2 = A2(e, `surface`), i2 = La(r2), a2 = i2.get(t4);
    if (a2 && !a2.disposed) throw tt(a2.label);
    let o2 = new Ga(r2.device, t4, n2, (e2) => {
      i2.get(e2.canvas) === e2 && i2.delete(e2.canvas), s3(), c2();
    }), s3 = Ma(r2).onAdvance(() => o2.applyAutoResize()), c2 = r2.own(`resource`, () => o2.dispose());
    return i2.set(t4, o2), o2;
  }
  var Ia = gt(`surfaces`);
  function La(e) {
    return e.service(Ia, () => /* @__PURE__ */ new Map());
  }
  var Ra = 0;
  var za = 0;
  function Ba() {
    return Ra > 0;
  }
  function Va() {
    return za > 0;
  }
  function Ha() {
    za += 1;
  }
  function Ua() {
    --za;
  }
  function Wa(e) {
    return e instanceof Ga;
  }
  var _e4, _t5, _n4, _r4, _i4, _a8, _o, _Ga_instances, s_fn2, c_fn2, l_fn2, u_fn, d_fn, _a9;
  var Ga = (_a9 = class {
    constructor(e, t4, n2, r2) {
      __privateAdd(this, _Ga_instances);
      __publicField(this, "device");
      __publicField(this, "canvas");
      __publicField(this, "options");
      __publicField(this, "unregister");
      __publicField(this, "resourceIdentity", h(`render-target`));
      __publicField(this, "label");
      __publicField(this, "context");
      __publicField(this, "autoResize");
      __publicField(this, "layoutBacked");
      __publicField(this, "format");
      __privateAdd(this, _e4, new g());
      __privateAdd(this, _t5, /* @__PURE__ */ new Set());
      __privateAdd(this, _n4, /* @__PURE__ */ new Set());
      __privateAdd(this, _r4);
      __privateAdd(this, _i4);
      __privateAdd(this, _a8, false);
      __privateAdd(this, _o, false);
      this.device = e, this.canvas = t4, this.options = n2, this.unregister = r2, this.label = n2.label, __privateSet(this, _i4, n2.clearColor === void 0 ? Gi : Ki(n2.clearColor, `surface.clearColor`));
      let i2 = t4.getContext(`webgpu`);
      if (!i2) throw et();
      if (this.context = i2, this.layoutBacked = Ka(t4), n2.autoResize === true && !this.layoutBacked) throw rt();
      this.autoResize = n2.autoResize ?? (!n2.size && this.layoutBacked), __privateSet(this, _r4, Qa(n2.dpr)), this.format = n2.format ?? $a();
      let a2 = qa(t4, n2, this.layoutBacked, __privateGet(this, _r4));
      (n2.size || this.layoutBacked) && Xa(t4, a2), i2.configure({ device: e.gpu, format: this.format, alphaMode: n2.alphaMode ?? `premultiplied`, colorSpace: n2.colorSpace ?? `srgb`, usage: eo() });
    }
    get gpu() {
      return this.context;
    }
    get size() {
      return __privateMethod(this, _Ga_instances, d_fn).call(this), Ya(this.canvas);
    }
    get texelSize() {
      let e = this.size;
      return [1 / e[0], 1 / e[1]];
    }
    get color() {
      return __privateMethod(this, _Ga_instances, d_fn).call(this), new U(this.device, this.context.getCurrentTexture(), { kind: `2d`, size: this.size, format: this.format, usage: [`render_attachment`, `texture_binding`, `copy_src`], label: this.options.label ? `${this.options.label}.color` : `surface.color` }, `external`);
    }
    get colors() {
      return [this.color];
    }
    get depth() {
      __privateMethod(this, _Ga_instances, d_fn).call(this);
    }
    get sampleCount() {
      return __privateMethod(this, _Ga_instances, d_fn).call(this), 1;
    }
    get dpr() {
      return __privateGet(this, _r4);
    }
    get clearColor() {
      return qi(__privateGet(this, _i4));
    }
    set clearColor(e) {
      __privateSet(this, _i4, Ki(e, `surface.clearColor`));
    }
    get disposed() {
      return __privateGet(this, _a8);
    }
    resize(e) {
      if (__privateMethod(this, _Ga_instances, d_fn).call(this), __privateGet(this, _o)) throw it(this.options.label);
      __privateMethod(this, _Ga_instances, s_fn2).call(this, Za(e), __privateGet(this, _r4), true);
    }
    applyAutoResize() {
      if (__privateGet(this, _a8) || !this.autoResize || !this.layoutBacked) return;
      let e = Qa(this.options.dpr), t4 = Ja(this.canvas, e);
      __privateMethod(this, _Ga_instances, s_fn2).call(this, t4, e, true);
    }
    onResize(e) {
      __privateMethod(this, _Ga_instances, d_fn).call(this), __privateGet(this, _t5).add(e), __privateSet(this, _o, true), Ra += 1;
      try {
        e(__privateMethod(this, _Ga_instances, u_fn).call(this));
      } finally {
        --Ra, __privateSet(this, _o, false);
      }
      return () => {
        __privateGet(this, _t5).delete(e);
      };
    }
    onDestroy(e) {
      return __privateMethod(this, _Ga_instances, d_fn).call(this), __privateGet(this, _e4).onDestroy(this, e);
    }
    onTexturesRecreated(e) {
      return __privateMethod(this, _Ga_instances, d_fn).call(this), __privateGet(this, _n4).add(e), () => {
        __privateGet(this, _n4).delete(e);
      };
    }
    renderPassDescriptor(e = {}) {
      let { clear: t4 = [0, 0, 0, 1], preserve: n2 } = e;
      __privateMethod(this, _Ga_instances, d_fn).call(this);
      let r2 = { view: this.context.getCurrentTexture().createView(), loadOp: n2 ? `load` : `clear`, storeOp: `store` };
      return n2 || (r2.clearValue = na(t4)), { colorAttachments: [r2] };
    }
    dispose() {
      if (!__privateGet(this, _a8)) {
        __privateSet(this, _a8, true);
        try {
          this.context.unconfigure?.();
        } catch {
        }
        this.unregister(this), __privateGet(this, _t5).clear(), __privateGet(this, _n4).clear(), __privateGet(this, _e4).emit(this);
      }
    }
  }, _e4 = new WeakMap(), _t5 = new WeakMap(), _n4 = new WeakMap(), _r4 = new WeakMap(), _i4 = new WeakMap(), _a8 = new WeakMap(), _o = new WeakMap(), _Ga_instances = new WeakSet(), s_fn2 = function(e, t4, n2) {
    let r2 = !ra(Ya(this.canvas), e);
    __privateSet(this, _r4, t4), r2 && (Xa(this.canvas, e), __privateMethod(this, _Ga_instances, c_fn2).call(this), n2 && __privateMethod(this, _Ga_instances, l_fn2).call(this));
  }, c_fn2 = function() {
    for (let e of [...__privateGet(this, _n4)]) e();
  }, l_fn2 = function() {
    __privateSet(this, _o, true), Ra += 1;
    try {
      let e = __privateMethod(this, _Ga_instances, u_fn).call(this);
      for (let t4 of [...__privateGet(this, _t5)]) t4(e);
    } finally {
      --Ra, __privateSet(this, _o, false);
    }
  }, u_fn = function() {
    let e = Ya(this.canvas);
    return { width: e[0], height: e[1], dpr: __privateGet(this, _r4), surface: this };
  }, d_fn = function() {
    if (__privateGet(this, _a8)) throw nt(this.options.label);
  }, _a9);
  function Ka(e) {
    return typeof e.clientWidth == `number`;
  }
  function qa(e, t4, n2, r2) {
    return t4.size ? Za(t4.size) : n2 ? Ja(e, r2) : Za(Ya(e));
  }
  function Ja(e, t4) {
    let n2 = e;
    return Za([Math.round(n2.clientWidth * t4), Math.round(n2.clientHeight * t4)]);
  }
  function Ya(e) {
    let t4 = e;
    return [t4.width, t4.height];
  }
  function Xa(e, t4) {
    let n2 = e;
    n2.width = t4[0], n2.height = t4[1];
  }
  function Za(e) {
    return [Math.max(1, Math.floor(e[0])), Math.max(1, Math.floor(e[1]))];
  }
  function Qa(e) {
    let t4 = globalThis.devicePixelRatio ?? 1;
    return Array.isArray(e) ? Math.min(e[1], Math.max(e[0], t4)) : typeof e == `number` ? e : t4;
  }
  function $a() {
    return globalThis.navigator?.gpu?.getPreferredCanvasFormat?.() ?? `bgra8unorm`;
  }
  function eo() {
    let e = globalThis.GPUTextureUsage;
    return e ? e.RENDER_ATTACHMENT | e.TEXTURE_BINDING | e.COPY_SRC : void 0;
  }
  var to = { drawIndirect: { bytes: 16, args: `4 u32 values: vertexCount, instanceCount, firstVertex, firstInstance` }, drawIndexedIndirect: { bytes: 20, args: `5 32-bit values: indexCount, instanceCount, firstIndex, baseVertex (signed), firstInstance` }, dispatchWorkgroupsIndirect: { bytes: 12, args: `3 u32 values: workgroupCountX, workgroupCountY, workgroupCountZ` } };
  function no(e, t4, n2, r2) {
    let i2 = typeof n2 == `object` && n2 ? n2.buffer : void 0, o2 = ro(n2) ? n2 : ro(i2) ? i2 : void 0;
    if (!o2) throw Ae(e, `received ${io(n2)}; expected a StorageBuffer or { buffer, offset? }.`, t4);
    let s3 = o2 === n2 ? 0 : n2.offset ?? 0;
    if (typeof s3 != `number` || !Number.isInteger(s3) || s3 < 0) throw Ae(e, `offset must be an integer >= 0; received ${io(s3)}.`, t4);
    if (s3 % 4 != 0) throw Ae(e, `offset must be a multiple of 4 (WebGPU requires "indirectOffset is a multiple of 4"); received ${s3}.`, t4);
    if (!o2.buffer.options.usage.includes(`indirect`)) throw Ae(e, `the buffer lacks the "indirect" usage (WebGPU requires "indirectBuffer.usage contains INDIRECT"); create it with storage(gpu, ${o2.size}, { indirect: true }).`, t4);
    let { bytes: c2, args: l2 } = to[r2];
    if (s3 + c2 > o2.size) throw Ae(e, `${r2} reads ${c2} bytes (${l2}) at offset ${s3}, but offset + ${c2} = ${s3 + c2} exceeds the buffer size ${o2.size}.`, t4);
    return { buffer: o2.gpu, offset: s3 };
  }
  function ro(e) {
    return typeof e == `object` && !!e && `gpu` in e && `size` in e && e.buffer instanceof _;
  }
  function io(e) {
    if (typeof e == `string`) return `"${e}"`;
    try {
      return JSON.stringify(e) ?? String(e);
    } catch {
      return String(e);
    }
  }
  var ao = /* @__PURE__ */ Symbol(`vgpu.frame.drawable`);
  function oo(e) {
    return e?.[ao];
  }
  var so = /* @__PURE__ */ Symbol(`vgpu.frame.bundle`);
  function co(e) {
    return e?.[so];
  }
  var lo = /* @__PURE__ */ Symbol(`vgpu.frame.passAttachment`);
  function uo(e) {
    return typeof e?.[lo] == `function` ? e : void 0;
  }
  function fo(e, t4) {
    return _o2(A2(e, `sampler`)).sampler(t4);
  }
  var po = 1;
  function mo(e) {
    let t4 = /* @__PURE__ */ new Map(), n2 = /* @__PURE__ */ new WeakMap();
    return { sampler(r2 = {}) {
      let i2 = ho(r2), a2 = t4.get(i2);
      return a2 || (a2 = e.gpu.createSampler(r2), t4.set(i2, a2), n2.set(a2, { kind: `sampler`, id: po++ })), a2;
    }, identity(e2) {
      let t5 = n2.get(e2);
      return t5 || (t5 = { kind: `sampler`, id: po++ }, n2.set(e2, t5)), t5;
    } };
  }
  function ho(e) {
    if (typeof e != `object` || !e) return JSON.stringify(e);
    if (Array.isArray(e)) return `[${e.map(ho).join(`,`)}]`;
    let t4 = e;
    return `{${Object.keys(t4).sort().map((e2) => `${JSON.stringify(e2)}:${ho(t4[e2])}`).join(`,`)}}`;
  }
  var go = gt(`render-service`);
  function _o2(e) {
    return e.service(go, vo);
  }
  function vo(e) {
    let t4 = e.device, n2 = ur(), r2 = Sa(t4, { errorSink: (t5) => e.reportError(t5), registerSettledSource: (t5) => e.registerSettledSource(t5) }), i2 = ba(t4), a2 = xa(t4), o2 = mo(t4);
    return e.own(`service`, () => {
      r2.dispose(), i2.dispose(), a2.dispose(), n2.dispose();
    }), { binds: n2, pipelines: r2, shaderModules: i2, pipelineLayouts: a2, sampler: (e2) => o2.sampler(e2) };
  }
  function yo(e) {
    if (typeof e == `string`) return e;
    if (!bo(e) || !(`version` in e) || e.version !== 1) throw pt(e);
    let t4 = e.wgsl;
    if (typeof t4 != `string`) throw pt(e);
    return t4;
  }
  function bo(e) {
    return typeof e == `object` && !!e;
  }
  function xo(e, t4) {
    let n2 = A2(e, `draw`), r2 = _o2(n2), i2 = yo(t4.shader);
    return new wo(n2.device, i2, { ...t4, shader: i2 }, r2.binds, void 0, r2.pipelines, r2.shaderModules, r2.pipelineLayouts, (e2) => n2.reportError(e2), (e2) => {
      n2.trackDelivery(e2);
    });
  }
  var So = 1;
  var Co = /* @__PURE__ */ new WeakMap();
  var _e5, _wo_instances, t_fn, n_fn, r_fn, i_fn, a_fn2, o_fn2, s_fn3, c_fn3, l_fn3, _a10;
  var wo = (_a10 = class {
    constructor(e, t4, n2, r2 = ur(), i2, a2 = Sa(e), o2 = ba(e), s3 = xa(e), c2, l2) {
      __privateAdd(this, _wo_instances);
      __publicField(this, "source");
      __publicField(this, "label");
      __privateAdd(this, _e5, /* @__PURE__ */ new Map());
      this.source = t4, k2(e, `Draw.constructor`), this.label = n2.label ?? `draw`;
      let u3 = So++, d2 = lr(t4, `${this.label}.wgsl`), f2 = Vo(this.label, n2.entry), p2 = pa(this.label, d2.entryPoints, `vertex`, f2.vertex, `draw`), m2 = pa(this.label, d2.entryPoints, `fragment`, f2.fragment, `draw`), h2 = Ho(d2, p2, m2), g2 = [p2, m2].filter((e2) => !!e2), _2 = Si(d2.bindings, g2);
      To(e, this.label, d2.bindings, g2, _2);
      let v2 = n2.geometry, y2 = p2 ? fr(p2, `inputs`, this.label) : [], b2 = v2 && Ye2 in v2 ? v2[Ye2](y2, `${this.label}.geometry`) : v2?.vertexBufferLayouts, x2 = new Map(wi(e, this.label, d2, _2)), ee2 = s3.get(x2), S2 = o2.get(t4, `${this.label}.shader`), C2 = vs(), te2 = Mo(this.label, n2), w2 = Ro(this.label, n2, te2), ne2 = Uo(e, this.label, n2), re2 = Qo(e, this.label, n2), ie2 = ns(this.label, n2), ae2 = cs(this.label, n2), oe2 = ha(this.label, n2.constants, d2.overrides, `draw`), T2 = Ai({ device: e, label: this.label, drawId: u3, reflection: d2, bindGroupLayouts: x2, cache: r2, onIdentityChange: (e2) => C2.markStale({ kind: `binding-identity`, drawLabel: this.label, ...e2 }) });
      Co.set(this, { id: u3, device: e, opts: n2, vertexBufferLayouts: b2, cache: r2, defaultTarget: i2, reflection: d2, visibility: _2, vertexEntry: p2?.name ?? `vs_main`, fragmentEntry: m2?.name ?? `fs_main`, entryKey: h2, setCore: T2, bindGroupLayouts: x2, pipelineLayout: ee2, shaderModule: S2, pipelineStore: a2, pipelineLayouts: s3, errorSink: c2, trackSettled: l2, resolvedPipelineKeys: /* @__PURE__ */ new Set(), recordedIn: C2, ...te2, ...w2, ...ne2, ...re2, ...ie2, ...ae2, ...oe2 }), n2.set && this.set(n2.set);
      for (let e2 of n2.targets ?? []) this.compileSync(e2);
    }
    get gpu() {
      let e = X2(this);
      for (let t4 of e.resolvedPipelineKeys) {
        let n2 = e.pipelineStore.getReady(t4);
        if (n2) return n2;
      }
    }
    get targets() {
      return X2(this).opts.targets;
    }
    get [ao]() {
      return this;
    }
    writesDepth() {
      return ms(this);
    }
    stencilWritingOps() {
      return hs(this);
    }
    set(e) {
      let t4 = X2(this);
      k2(t4.device, `${this.label}.set`);
      for (let n2 of t4.setCore.set(e)) t4.recordedIn.markStale({ kind: `binding-identity`, drawLabel: this.label, ...n2 });
      return this;
    }
    group(e, t4) {
      let n2 = X2(this);
      k2(n2.device, `${this.label}.group`);
      let r2 = __privateGet(this, _e5).get(e) ?? this.layout(e), i2 = n2.setCore.claimGroup(e, t4, r2);
      return n2.recordedIn.markStale({ kind: `group-claim`, drawLabel: this.label, group: e, previousIdentity: i2, newIdentity: `claimed-group:${e}` }), this;
    }
    layout(e, t4 = {}) {
      return k2(X2(this).device, `${this.label}.layout`), t4.dynamicOffsets ? __privateMethod(this, _wo_instances, t_fn).call(this, e) : X2(this).setCore.layout(e);
    }
    draw(e = {}) {
      k2(X2(this).device, `${this.label}.draw`);
      let t4 = ia(e) ? { target: e } : e, n2 = X2(this), r2 = t4.target ?? n2.defaultTarget;
      if (!r2) throw We(`${this.label}.draw`);
      Ss(r2, `${this.label}.draw`);
      let i2 = n2.device.gpu.createCommandEncoder(), a2 = i2.beginRenderPass(r2.renderPassDescriptor()), o2 = [];
      try {
        this.encode(a2, r2, t4, (e2) => o2.push(e2));
      } catch (e2) {
        U2(o2), _r2(n2.device);
        try {
          a2.end();
        } catch {
        }
        throw e2;
      }
      Tr(n2.device, a2, o2, o2[0]?.context);
      let s3, c2 = o2[0]?.context;
      c2 && mr(n2.device, c2);
      try {
        s3 = i2.finish();
      } catch (e2) {
        let t5 = c2 ? H2(n2.device) : void 0;
        U2(o2), t5 && U2([t5]);
        let r3 = t5?.context ?? c2;
        if (r3) {
          _s(n2, r3.label, r3.group, e2);
          return;
        }
        throw e2;
      }
      if (c2) {
        let e2 = H2(n2.device);
        e2 && (o2[0] = o2[0] ? br(e2, o2[0]) : e2);
      }
      let l2 = o2[0]?.context;
      l2 && mr(n2.device, l2);
      try {
        n2.device.gpu.queue.submit([s3]);
      } catch (e2) {
        let t5 = l2 ? H2(n2.device) : void 0;
        U2(o2), t5 && U2([t5]);
        let r3 = t5?.context ?? l2;
        if (r3) {
          _s(n2, r3.label, r3.group, e2);
          return;
        }
        throw e2;
      }
      if (l2) {
        let e2 = H2(n2.device);
        e2 && (o2[0] = o2[0] ? br(e2, o2[0]) : e2);
      }
      if (o2.length) {
        let e2 = yr(n2.device, o2, { errorSink: n2.errorSink });
        n2.trackSettled?.(e2);
      }
    }
    encode(e, t4, n2 = {}, r2) {
      k2(X2(this).device, `${this.label}.encode`), X2(this).setCore.assertUsable();
      let i2 = this.pipelineFor(t4, true);
      if (!i2) return;
      e.setPipeline(i2);
      let a2 = X2(this);
      a2.blendConstant && e.setBlendConstant(a2.blendConstant), a2.stencilRef !== void 0 && e.setStencilReference(a2.stencilRef);
      for (let t5 of a2.setCore.bindGroups()) __privateMethod(this, _wo_instances, n_fn).call(this, e, t5, n2, r2);
      __privateMethod(this, _wo_instances, o_fn2).call(this, e, n2);
    }
    compile(e) {
      k2(X2(this).device, `${this.label}.compile`);
      let { key: t4, signature: n2, signatureKey: r2 } = __privateMethod(this, _wo_instances, r_fn).call(this, e, `${this.label}.compile`);
      return X2(this).pipelineStore.getAsync(t4, () => __privateMethod(this, _wo_instances, l_fn3).call(this, n2), { where: `${this.label}.compile`, signature: r2 }).then(() => (k2(X2(this).device, `${this.label}.compile`), X2(this).resolvedPipelineKeys.add(t4), this));
    }
    compileSync(e) {
      k2(X2(this).device, `${this.label}.compileSync`);
      let { key: t4, signature: n2, signatureKey: r2 } = __privateMethod(this, _wo_instances, r_fn).call(this, e, `${this.label}.compileSync`);
      return X2(this).pipelineStore.getSync(t4, () => __privateMethod(this, _wo_instances, c_fn3).call(this, n2), { where: `${this.label}.compileSync`, signature: r2 }) && X2(this).resolvedPipelineKeys.add(t4), this;
    }
    pipelineFor(e, t4 = false) {
      k2(X2(this).device, `${this.label}.pipelineFor`);
      let { key: n2, signature: r2, signatureKey: i2 } = __privateMethod(this, _wo_instances, r_fn).call(this, e, `${this.label}.pipelineFor`, t4), a2 = X2(this).pipelineStore.getSync(n2, () => __privateMethod(this, _wo_instances, c_fn3).call(this, r2), { where: `${this.label}.pipelineFor`, signature: i2 });
      return a2 && X2(this).resolvedPipelineKeys.add(n2), a2;
    }
    pipelineForAsync(e) {
      k2(X2(this).device, `${this.label}.pipelineForAsync`);
      let { key: t4, signature: n2, signatureKey: r2 } = __privateMethod(this, _wo_instances, r_fn).call(this, e, `${this.label}.pipelineForAsync`);
      return X2(this).pipelineStore.getAsync(t4, () => __privateMethod(this, _wo_instances, l_fn3).call(this, n2), { where: `${this.label}.pipelineForAsync`, signature: r2 }).then((e2) => (k2(X2(this).device, `${this.label}.pipelineForAsync`), X2(this).resolvedPipelineKeys.add(t4), e2));
    }
  }, _e5 = new WeakMap(), _wo_instances = new WeakSet(), t_fn = function(e) {
    let t4 = X2(this);
    t4.setCore.layout(e);
    let n2 = __privateGet(this, _e5).get(e);
    if (n2) return n2;
    let r2 = bs(this, e), i2 = Ei(t4.device, `${this.label}.group${e}.dynamic.bgl`, r2);
    return __privateGet(this, _e5).set(e, i2), t4.bindGroupLayouts.set(e, i2), t4.pipelineLayout = t4.pipelineLayouts.get(t4.bindGroupLayouts), i2;
  }, n_fn = function(e, t4, n2, r2) {
    let i2 = ys(n2.offsets, t4.group, t4.offsets);
    if (!t4.claimValidation || !r2) {
      e.setBindGroup(t4.group, t4.bindGroup, i2);
      return;
    }
    mr(X2(this).device, t4.claimValidation);
    try {
      e.setBindGroup(t4.group, t4.bindGroup, i2);
    } catch (e2) {
      throw gr(X2(this).device), _e(t4.claimValidation.label, t4.claimValidation.group, e2);
    }
    let a2 = H2(X2(this).device);
    a2 && r2(a2);
  }, r_fn = function(e, t4, n2 = false) {
    let r2 = __privateMethod(this, _wo_instances, i_fn).call(this, e, t4, n2);
    return { signature: r2, signatureKey: ua(r2), key: __privateMethod(this, _wo_instances, a_fn2).call(this, r2) };
  }, i_fn = function(e, t4, r2 = false) {
    let i2 = X2(this), a2 = e ?? i2.defaultTarget;
    if (!a2) throw We(t4);
    r2 || Ss(a2, t4);
    let o2 = la(a2);
    if (da(o2, t4), i2.colorStates && i2.colorStates.length !== o2.colors.length) throw xe(this.label, `expected one entry per color attachment; colors has ${i2.colorStates.length}, but the target signature has ${o2.colors.length}.`, t4);
    if (i2.multisampleState?.alphaToCoverageEnabled && (o2.sampleCount ?? 1) <= 1) throw De(this.label, `alphaToCoverage requires a multisampled target, but the target signature has sampleCount ${o2.sampleCount ?? 1}; create the target with msaa: true.`, t4);
    if ((i2.stencilState || i2.stencilRef !== void 0) && !ta(o2.depth)) throw Ee(this.label, `stencil requires a depth format with a stencil aspect, but the target signature has ${o2.depth ? `"${o2.depth}"` : `no depth`}; create the target with depth: "depth24plus-stencil8".`, t4);
    return o2;
  }, a_fn2 = function(e) {
    let t4 = X2(this), n2 = t4.opts.geometry;
    return fa({ module: t4.shaderModule, pipelineLayout: t4.pipelineLayout, vertexBufferLayouts: t4.vertexBufferLayouts, signature: e, fragmentKey: t4.fragmentKey, topology: n2?.topology, stripIndexFormat: ko(n2), cullMode: t4.cullMode, frontFace: t4.frontFace, unclippedDepth: t4.unclippedDepth, depthKey: t4.depthKey, stencilKey: t4.stencilKey, multisampleKey: t4.multisampleKey, constantsKey: t4.constantsKey, entryKey: t4.entryKey });
  }, o_fn2 = function(e, t4 = {}) {
    let n2 = X2(this).opts.geometry;
    if (n2?.vertexBuffers && n2.vertexBuffers.forEach((t5, n3) => e.setVertexBuffer(n3, t5)), t4.indirect !== void 0) return __privateMethod(this, _wo_instances, s_fn3).call(this, e, n2, t4);
    let r2 = Oo(this.label, n2, X2(this).opts, t4);
    if (!n2?.indexBuffer) return e.draw(r2.vertexCount, r2.instanceCount, r2.firstVertex, r2.firstInstance);
    e.setIndexBuffer(n2.indexBuffer, n2.indexFormat ?? `uint32`), e.drawIndexed(r2.indexCount, r2.instanceCount, r2.firstIndex, r2.baseVertex, r2.firstInstance);
  }, s_fn3 = function(e, t4, n2) {
    let r2 = `${this.label}.draw`, i2 = Eo.find((e2) => n2[e2] !== void 0);
    if (i2 !== void 0) throw Ae(this.label, `indirect cannot be combined with ${i2} in the same call; the GPU reads the draw arguments from the buffer, so the CPU-side value would be ignored.`, r2);
    let o2 = !!t4?.indexBuffer, { buffer: s3, offset: c2 } = no(this.label, r2, n2.indirect, o2 ? `drawIndexedIndirect` : `drawIndirect`);
    if (!o2) return e.drawIndirect(s3, c2);
    e.setIndexBuffer(t4.indexBuffer, t4.indexFormat ?? `uint32`), e.drawIndexedIndirect(s3, c2);
  }, c_fn3 = function(e) {
    let t4 = X2(this);
    return t4.device.gpu.createRenderPipeline({ label: `${this.label}.pipeline`, layout: t4.pipelineLayout, vertex: { module: t4.shaderModule, entryPoint: t4.vertexEntry, buffers: [...t4.vertexBufferLayouts ?? []], ...t4.constants ? { constants: t4.constants } : {} }, fragment: { module: t4.shaderModule, entryPoint: t4.fragmentEntry, targets: Do(e, t4), ...t4.constants ? { constants: t4.constants } : {} }, primitive: Ao(t4.opts.geometry, t4.cullMode, t4.frontFace, t4.unclippedDepth), depthStencil: Zo(e, t4), multisample: ss(e, t4) });
  }, l_fn3 = function(e) {
    let t4 = X2(this);
    return t4.device.gpu.createRenderPipelineAsync({ label: `${this.label}.pipeline`, layout: t4.pipelineLayout, vertex: { module: t4.shaderModule, entryPoint: t4.vertexEntry, buffers: [...t4.vertexBufferLayouts ?? []], ...t4.constants ? { constants: t4.constants } : {} }, fragment: { module: t4.shaderModule, entryPoint: t4.fragmentEntry, targets: Do(e, t4), ...t4.constants ? { constants: t4.constants } : {} }, primitive: Ao(t4.opts.geometry, t4.cullMode, t4.frontFace, t4.unclippedDepth), depthStencil: Zo(e, t4), multisample: ss(e, t4) });
  }, _a10);
  function To(e, t4, n2, r2, i2) {
    let a2 = e.limits;
    for (let [e2, o2, s3] of [[`vertex`, 1, `maxStorageBuffersInVertexStage`], [`fragment`, 2, `maxStorageBuffersInFragmentStage`]]) {
      let c2 = r2.find((t5) => t5.stage === e2);
      if (!c2) continue;
      let l2 = n2.filter((e3) => e3.bindingLayout?.kind === `buffer` && e3.bindingLayout.buffer.type !== `uniform` && i2(e3) & o2), u3 = a2[s3] ?? a2.maxStorageBuffersPerShaderStage;
      if (u3 !== void 0 && l2.length > u3) throw de(t4, e2, c2.name, l2.length, u3, l2);
    }
  }
  var Eo = [`vertices`, `indices`, `instances`, `firstVertex`, `firstIndex`, `baseVertex`, `firstInstance`];
  function Do(e, t4) {
    return e.colors.map((e2, n2) => {
      let r2 = t4.colorStates?.[n2], i2 = r2?.blendState ?? t4.blendState, a2 = r2?.writeMask ?? t4.writeMask, o2 = { format: e2 };
      return i2 && (o2.blend = i2), a2 !== void 0 && (o2.writeMask = a2), o2;
    });
  }
  function Oo(e, t4, n2, r2) {
    J2(e, `DrawOptions.instances`, n2.instances), J2(e, `DrawOptions.vertices`, n2.vertices), J2(e, `DrawOptions.firstInstance`, n2.firstInstance), J2(e, `DrawCallOptions.instances`, r2.instances), q2(e, `DrawCallOptions.vertices`, r2.vertices), q2(e, `DrawCallOptions.indices`, r2.indices), q2(e, `DrawCallOptions.firstVertex`, r2.firstVertex), q2(e, `DrawCallOptions.firstIndex`, r2.firstIndex), q2(e, `DrawCallOptions.baseVertex`, r2.baseVertex), J2(e, `DrawCallOptions.firstInstance`, r2.firstInstance), J2(e, `GeometryLike.vertexCount`, t4?.vertexCount), J2(e, `GeometryLike.indexCount`, t4?.indexCount), J2(e, `GeometryLike.instanceCount`, t4?.instanceCount), q2(e, `GeometryLike.firstVertex`, t4?.firstVertex), q2(e, `GeometryLike.firstIndex`, t4?.firstIndex), q2(e, `GeometryLike.baseVertex`, t4?.baseVertex);
    let i2 = !!t4?.indexBuffer, a2 = t4?.geometry ?? (t4 && Ye2 in t4 ? t4 : void 0), o2 = r2.firstVertex ?? t4?.firstVertex ?? 0, s3 = r2.vertices ?? t4?.vertexCount ?? n2.vertices ?? 3, c2 = r2.firstIndex ?? t4?.firstIndex ?? 0, l2 = r2.indices ?? t4?.indexCount ?? 0, u3 = r2.baseVertex ?? t4?.baseVertex ?? 0;
    if (i2) jo(e, `index`, c2, l2, a2?.indexCount);
    else if (r2.indices !== void 0 || r2.firstIndex !== void 0 || r2.baseVertex !== void 0) throw Ke(`${e}.draw`, `Index range needs an indexed geometry.`);
    return i2 || jo(e, `vertex`, o2, s3, a2?.vertexCount), { instanceCount: r2.instances ?? n2.instances ?? t4?.instanceCount ?? 1, firstInstance: r2.firstInstance ?? n2.firstInstance ?? 0, vertexCount: s3, firstVertex: o2, indexCount: l2, firstIndex: c2, baseVertex: u3 };
  }
  function ko(e) {
    let t4 = e?.topology ?? `triangle-list`;
    return e?.stripIndexFormat ?? (t4.endsWith(`strip`) ? e?.indexFormat : void 0);
  }
  function Ao(e, t4, n2, r2) {
    let i2 = e?.topology ?? `triangle-list`, a2 = ko(e), o2 = a2 ? { topology: i2, stripIndexFormat: a2 } : { topology: i2 };
    return t4 !== void 0 && (o2.cullMode = t4), n2 !== void 0 && (o2.frontFace = n2), r2 && (o2.unclippedDepth = true), o2;
  }
  function jo(e, t4, n2, r2, i2) {
    if (!(i2 === void 0 || n2 + r2 <= i2)) throw Ke(`${e}.draw`, `${t4} range [${n2}, ${n2 + r2}) exceeds parent geometry ${t4} count ${i2}.`);
  }
  function q2(e, t4, n2) {
    if (!(n2 === void 0 || Number.isInteger(n2) && n2 >= 0)) throw Ke(`${e}.draw`, `${t4} must be an integer >= 0; received ${String(n2)}.`);
  }
  function J2(e, t4, n2) {
    if (n2 !== void 0 && !(Number.isInteger(n2) && n2 >= 0)) throw new Y({ code: `VGPU-R1-DRAW-COUNT`, message: `${t4} of '${e}' must be an integer >= 0; received ${String(n2)}. Use 0 only when you want to issue a valid draw with no vertices/instances.`, where: `${e}.draw` });
  }
  function Mo(e, t4) {
    let n2 = t4.blend === void 0 ? void 0 : Po(e, t4.blend), r2 = t4.writeMask === void 0 ? void 0 : us(e, t4.writeMask), i2 = t4.colors === void 0 ? void 0 : No(e, t4.colors);
    return { blendState: n2, writeMask: r2, colorStates: i2, fragmentKey: i2 ? `${ds(n2, r2)}@${i2.map(ps).join(`@`)}` : n2 || r2 !== void 0 ? ds(n2, r2) : void 0 };
  }
  function No(e, t4) {
    if (!Array.isArray(t4)) throw xe(e, `colors must be an array; received ${Y2(t4)}.`);
    return t4.map((t5, n2) => {
      if (t5 == null) return null;
      if (typeof t5 != `object` || Array.isArray(t5)) throw xe(e, `colors[${n2}] must be null or { blend?, writeMask? }; received ${Y2(t5)}.`);
      let r2 = t5.blend === void 0 ? void 0 : Po(`${e}.colors[${n2}]`, t5.blend), i2 = t5.writeMask === void 0 ? void 0 : us(`${e}.colors[${n2}]`, t5.writeMask);
      return !r2 && i2 === void 0 ? null : { blendState: r2, writeMask: i2 };
    });
  }
  function Po(e, t4) {
    if (t4 === `alpha`) return Io({ src: `src-alpha`, dst: `one-minus-src-alpha` }, { src: `one`, dst: `one-minus-src-alpha` });
    if (t4 === `premultiplied`) return Io({ src: `one`, dst: `one-minus-src-alpha` }, { src: `one`, dst: `one-minus-src-alpha` });
    if (t4 === `additive`) return Io({ src: `one`, dst: `one` }, { src: `one`, dst: `one` });
    if (typeof t4 != `object` || !t4 || !Fo(t4.color)) throw ve(e, t4);
    let n2 = t4.color, r2 = t4.alpha;
    if (r2 !== void 0 && !Fo(r2)) throw ve(e, t4);
    return Io(n2, r2 ?? n2);
  }
  function Fo(e) {
    return typeof e == `object` && !!e && typeof e.src == `string` && typeof e.dst == `string`;
  }
  function Io(e, t4) {
    return { color: Lo(e), alpha: Lo(t4) };
  }
  function Lo(e) {
    return { srcFactor: e.src, dstFactor: e.dst, operation: e.op ?? `add` };
  }
  function Ro(e, t4, n2) {
    if (t4.blendConstant === void 0) return {};
    let r2 = t4.blendConstant;
    if (!Array.isArray(r2) || r2.length !== 4 || r2.some((e2) => typeof e2 != `number` || !Number.isFinite(e2))) throw ye(e, `received ${Y2(r2)}; expected [r, g, b, a] finite numbers.`);
    if (!zo(n2).some((e2) => e2 && Bo(e2))) throw ye(e, `no color target's effective blend uses a "constant"/"one-minus-constant" factor (colors[i].blend replaces the top-level blend for that target), so blendConstant would have no effect.`);
    return { blendConstant: { r: r2[0], g: r2[1], b: r2[2], a: r2[3] } };
  }
  function zo(e) {
    return e.colorStates ? e.colorStates.map((t4) => t4?.blendState ?? e.blendState) : [e.blendState];
  }
  function Bo(e) {
    return [e.color.srcFactor, e.color.dstFactor, e.alpha.srcFactor, e.alpha.dstFactor].some((e2) => e2 === `constant` || e2 === `one-minus-constant`);
  }
  function Vo(e, t4) {
    if (t4 === void 0) return {};
    if (typeof t4 != `object` || !t4 || Array.isArray(t4)) throw ke(e, `received ${Y2(t4)}; expected { vertex?, fragment? } entry point names.`);
    return t4;
  }
  function Ho(e, t4, n2) {
    let r2 = e.entryPoints.find((e2) => e2.stage === `vertex`), i2 = e.entryPoints.find((e2) => e2.stage === `fragment`);
    if (!(t4 === r2 && n2 === i2)) return `en~${t4?.name ?? ``}~${n2?.name ?? ``}`;
  }
  function Uo(e, t4, n2) {
    return { cullMode: n2.cull === void 0 ? void 0 : Go(t4, n2.cull), frontFace: n2.frontFace === void 0 ? void 0 : Ko(t4, n2.frontFace), unclippedDepth: n2.unclippedDepth === void 0 ? void 0 : Wo(e, t4, n2.unclippedDepth) };
  }
  function Wo(e, t4, n2) {
    if (typeof n2 != `boolean`) throw we(t4, `received ${Y2(n2)}; expected a boolean.`);
    if (n2) {
      if (!e.features.has(`depth-clip-control`)) throw we(t4, `the device lacks the "depth-clip-control" feature; request it at init: init({ requiredFeatures: ["depth-clip-control"] }) on an adapter that supports it.`);
      return true;
    }
  }
  function Go(e, t4) {
    if (t4 === `none` || t4 === `front` || t4 === `back`) return t4;
    throw Se(e, t4);
  }
  function Ko(e, t4) {
    if (t4 === `ccw` || t4 === `cw`) return t4;
    throw Ce(e, t4);
  }
  var qo = { depthWriteEnabled: true, depthCompare: `less-equal` };
  var Jo = [`never`, `less`, `equal`, `less-equal`, `greater`, `not-equal`, `greater-equal`, `always`];
  var Yo = -2147483648;
  var Xo = 2147483647;
  function Zo(e, t4) {
    if (e.depth) return { format: e.depth, ...t4.depthState ?? qo, ...t4.stencilState ?? {} };
  }
  function Qo(e, t4, n2) {
    if (n2.depth === void 0) return {};
    let r2 = $o(e, t4, n2.depth, n2.geometry?.topology ?? `triangle-list`);
    return { depthState: r2, depthKey: es(r2) };
  }
  function $o(e, t4, n2, r2) {
    if (n2 === false) return { depthWriteEnabled: false, depthCompare: `always` };
    if (typeof n2 != `object` || !n2) throw Te(t4, `received ${Y2(n2)}.`);
    if (n2.write !== void 0 && typeof n2.write != `boolean`) throw Te(t4, `write must be a boolean; received ${Y2(n2.write)}.`);
    if (n2.compare !== void 0 && !Jo.includes(n2.compare)) throw Te(t4, `compare must be a GPUCompareFunction; received ${Y2(n2.compare)}.`);
    if (n2.bias !== void 0 && !Number.isInteger(n2.bias)) throw Te(t4, `bias must be an integer (WebGPU depthBias is i32); received ${Y2(n2.bias)}.`);
    if (n2.bias !== void 0 && (n2.bias < Yo || n2.bias > Xo)) throw Te(t4, `bias must fit in the i32 range [${Yo}, ${Xo}] (WebGPU depthBias is i32); received ${Y2(n2.bias)}.`);
    if (n2.biasSlopeScale !== void 0 && !Number.isFinite(n2.biasSlopeScale)) throw Te(t4, `biasSlopeScale must be a finite number; received ${Y2(n2.biasSlopeScale)}.`);
    if (n2.biasClamp !== void 0 && !Number.isFinite(n2.biasClamp)) throw Te(t4, `biasClamp must be a finite number; received ${Y2(n2.biasClamp)}.`);
    let i2 = n2.bias ?? 0, a2 = n2.biasSlopeScale ?? 0, o2 = n2.biasClamp ?? 0;
    if ((i2 !== 0 || a2 !== 0 || o2 !== 0) && !r2.startsWith(`triangle`)) throw Te(t4, `bias, biasSlopeScale, and biasClamp must be 0 for "${r2}" topology.`);
    if (o2 !== 0 && e.isCompatibilityMode) throw Te(t4, `biasClamp must be 0 on a compatibility-mode device; received ${Y2(n2.biasClamp)}.`);
    return { depthWriteEnabled: n2.write ?? true, depthCompare: n2.compare ?? `less-equal`, ...i2 === 0 ? {} : { depthBias: i2 }, ...a2 === 0 ? {} : { depthBiasSlopeScale: a2 }, ...o2 === 0 ? {} : { depthBiasClamp: o2 } };
  }
  function es(e) {
    return `${+!!e.depthWriteEnabled}~${e.depthCompare}~${e.depthBias ?? 0}~${e.depthBiasSlopeScale ?? 0}~${e.depthBiasClamp ?? 0}`;
  }
  var ts = [`keep`, `zero`, `replace`, `invert`, `increment-clamp`, `decrement-clamp`, `increment-wrap`, `decrement-wrap`];
  function ns(e, t4) {
    if (t4.stencil === void 0) return {};
    let n2 = t4.stencil;
    if (typeof n2 != `object` || !n2 || Array.isArray(n2)) throw Ee(e, `received ${Y2(n2)}; expected { front?, back?, readMask?, writeMask?, ref? }.`);
    let r2 = n2.front === void 0 ? void 0 : rs(e, `front`, n2.front), i2 = n2.back === void 0 ? void 0 : rs(e, `back`, n2.back);
    is(e, `readMask`, n2.readMask), is(e, `writeMask`, n2.writeMask), is(e, `ref`, n2.ref);
    let a2 = { ...r2 ? { stencilFront: r2 } : {}, ...i2 ?? r2 ? { stencilBack: i2 ?? { ...r2 } } : {}, ...n2.readMask === void 0 ? {} : { stencilReadMask: n2.readMask }, ...n2.writeMask === void 0 ? {} : { stencilWriteMask: n2.writeMask } }, o2 = a2.stencilFront !== void 0 || a2.stencilBack !== void 0 || a2.stencilReadMask !== void 0 || a2.stencilWriteMask !== void 0;
    return !o2 && n2.ref === void 0 ? {} : { ...o2 ? { stencilState: a2, stencilKey: as(a2) } : {}, ...n2.ref === void 0 ? {} : { stencilRef: n2.ref } };
  }
  function rs(e, t4, n2) {
    if (typeof n2 != `object` || !n2 || Array.isArray(n2)) throw Ee(e, `${t4} must be a { compare?, fail?, depthFail?, pass? } object; received ${Y2(n2)}.`);
    if (n2.compare !== void 0 && !Jo.includes(n2.compare)) throw Ee(e, `${t4}.compare must be a GPUCompareFunction; received ${Y2(n2.compare)}.`);
    for (let [r2, i2] of [[`fail`, n2.fail], [`depthFail`, n2.depthFail], [`pass`, n2.pass]]) if (i2 !== void 0 && !ts.includes(i2)) throw Ee(e, `${t4}.${r2} must be a GPUStencilOperation; received ${Y2(i2)}.`);
    return { compare: n2.compare ?? `always`, failOp: n2.fail ?? `keep`, depthFailOp: n2.depthFail ?? `keep`, passOp: n2.pass ?? `keep` };
  }
  function is(e, t4, n2) {
    if (n2 !== void 0 && (typeof n2 != `number` || !Number.isInteger(n2) || n2 < 0 || n2 > 4294967295)) throw Ee(e, `${t4} must be an integer in [0, 0xFFFFFFFF] (WebGPU GPUStencilValue is u32); received ${Y2(n2)}.`);
  }
  function as(e) {
    return `st~${os(e.stencilFront)}~${os(e.stencilBack)}~${e.stencilReadMask ?? 4294967295}~${e.stencilWriteMask ?? 4294967295}`;
  }
  function os(e) {
    return e ? `${e.compare},${e.failOp},${e.depthFailOp},${e.passOp}` : `default`;
  }
  function ss(e, t4) {
    return { count: e.sampleCount ?? 1, ...t4.multisampleState ?? {} };
  }
  function cs(e, t4) {
    if (t4.multisample === void 0) return {};
    let r2 = t4.multisample;
    if (typeof r2 != `object` || !r2 || Array.isArray(r2)) throw De(e, `received ${Y2(r2)}; expected { alphaToCoverage?, mask? }.`);
    if (r2.alphaToCoverage !== void 0 && typeof r2.alphaToCoverage != `boolean`) throw De(e, `alphaToCoverage must be a boolean; received ${Y2(r2.alphaToCoverage)}.`);
    if (r2.mask !== void 0 && (typeof r2.mask != `number` || !Number.isInteger(r2.mask) || r2.mask < 0 || r2.mask > 4294967295)) throw De(e, `mask must be an integer in [0, 0xFFFFFFFF] (WebGPU GPUSampleMask is u32); received ${Y2(r2.mask)}.`);
    let i2 = { ...r2.alphaToCoverage === void 0 ? {} : { alphaToCoverageEnabled: r2.alphaToCoverage }, ...r2.mask === void 0 ? {} : { mask: r2.mask } };
    return i2.alphaToCoverageEnabled === void 0 && i2.mask === void 0 ? {} : { multisampleState: i2, multisampleKey: ls(i2) };
  }
  function ls(e) {
    return `ms~${+!!e.alphaToCoverageEnabled}~${e.mask ?? 4294967295}`;
  }
  function us(e, t4) {
    if (!Array.isArray(t4)) throw be(e, Y2(t4));
    let n2 = 0;
    for (let r2 of t4) if (r2 === `r`) n2 |= 1;
    else if (r2 === `g`) n2 |= 2;
    else if (r2 === `b`) n2 |= 4;
    else if (r2 === `a`) n2 |= 8;
    else throw be(e, Y2(r2));
    return n2;
  }
  function ds(e, t4) {
    return `${fs(e)};${t4 ?? 15}`;
  }
  function fs(e) {
    if (!e) return `none;none`;
    let t4 = e.color, n2 = e.alpha;
    return `${t4.srcFactor},${t4.dstFactor},${t4.operation};${n2.srcFactor},${n2.dstFactor},${n2.operation}`;
  }
  function ps(e) {
    return e ? `${e.blendState ? fs(e.blendState) : `inherit`};${e.writeMask ?? `inherit`}` : `inherit`;
  }
  function Y2(e) {
    if (typeof e == `string`) return `"${e}"`;
    try {
      return JSON.stringify(e) ?? String(e);
    } catch {
      return String(e);
    }
  }
  function ms(e) {
    return (X2(e).depthState ?? qo).depthWriteEnabled;
  }
  function hs(e) {
    let t4 = X2(e), n2 = t4.stencilState;
    if (!n2 || n2.stencilWriteMask === 0) return [];
    let r2 = t4.cullMode ?? `none`, i2 = [], a2 = (e2, t5) => {
      if (t5) for (let [n3, r3] of [[`fail`, t5.failOp], [`depthFail`, t5.depthFailOp], [`pass`, t5.passOp]]) r3 !== void 0 && r3 !== `keep` && i2.push(`${e2}.${n3}: "${r3}"`);
    };
    return r2 !== `front` && a2(`front`, n2.stencilFront), r2 !== `back` && a2(`back`, n2.stencilBack), i2;
  }
  function gs(e, t4, n2, r2 = {}, i2) {
    e.encode(t4, n2, r2, i2);
  }
  function X2(e) {
    let t4 = Co.get(e);
    if (!t4) throw TypeError(`Invalid Draw instance`);
    return t4;
  }
  function _s(e, t4, n2, r2) {
    let i2 = (async () => {
      await vr(e.device), k2(e.device, `${t4}.validation`);
      let i3 = _e(t4, n2, r2);
      e.errorSink ? await e.errorSink(i3) : console.error(i3);
    })();
    return e.trackSettled?.(i2), i2;
  }
  function vs() {
    let e = /* @__PURE__ */ new Set();
    return { add(t4) {
      e.add(t4);
    }, delete(t4) {
      e.delete(t4);
    }, list() {
      return [...e];
    }, markStale(t4) {
      for (let n2 of e) n2.markStale(t4);
    } };
  }
  function ys(e, t4, n2) {
    return e ? Array.isArray(e) ? e : e[t4] ?? n2 : n2;
  }
  function bs(e, t4) {
    let n2 = X2(e);
    return Ci(n2.reflection.bindings, t4, n2.visibility).map(xs);
  }
  function xs(e) {
    return e.buffer ? { ...e, buffer: { ...e.buffer, hasDynamicOffset: true } } : e;
  }
  function Ss(e, t4) {
    if (Wa(e) && !Va()) throw $e(t4);
  }
  function Cs(e, t4, n2 = {}) {
    if (`geometry` in n2) throw X(`effect`, `effect() never accepts vertex buffers; use draw(gpu, { shader, geometry: geometry(gpu, descriptor) }).`);
    let r2 = A2(e, `effect`), i2 = _o2(r2);
    return new Ts(r2.device, yo(t4), n2, i2.binds, void 0, i2.pipelines, i2.shaderModules, i2.pipelineLayouts, (e2) => r2.reportError(e2), (e2) => {
      r2.trackDelivery(e2);
    });
  }
  var ws = /* @__PURE__ */ new WeakMap();
  var Ts = class {
    get gpu() {
      return Es(this).gpu;
    }
    constructor(e, t4, n2 = {}, r2, i2, a2, o2, s3, c2, l2) {
      let u3 = Ds(t4), d2 = new wo(e, u3, { shader: u3, set: n2.set, label: n2.label ?? `effect`, blend: n2.blend, writeMask: n2.writeMask }, r2, i2, a2, o2, s3, c2, l2);
      ws.set(this, d2);
    }
    set(e) {
      return Es(this).set(e), this;
    }
    draw(e = {}) {
      Es(this).draw(ia(e) ? { target: e } : e);
    }
    compile(e) {
      return Es(this).compile(e).then(() => this);
    }
    compileSync(e) {
      return Es(this).compileSync(e), this;
    }
    encode(e, t4, n2 = {}, r2) {
      gs(Es(this), e, t4, n2, r2);
    }
    get [ao]() {
      return Es(this)[ao];
    }
  };
  function Es(e) {
    let t4 = ws.get(e);
    if (!t4) throw TypeError(`Invalid Effect instance`);
    return t4;
  }
  function Ds(e) {
    return Os(e) ? e : `
struct VgpuFullscreenVertexOut {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
};
@vertex fn vgpu_fullscreen_vs(@builtin(vertex_index) vi: u32) -> VgpuFullscreenVertexOut {
  var pos = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  var uv = array<vec2f, 3>(vec2f(0.0, 1.0), vec2f(2.0, 1.0), vec2f(0.0, -1.0));
  var out: VgpuFullscreenVertexOut;
  out.position = vec4f(pos[vi], 0.0, 1.0);
  out.uv = uv[vi];
  return out;
}
${e}`;
  }
  function Os(e) {
    return lr(e, `effect.wgsl`).entryPoints.some((e2) => e2.stage === `vertex`);
  }
  var ks = gt(`clock`);
  function As(e) {
    return js(A2(e, `clock`));
  }
  function js(e) {
    return e.service(ks, (e2) => {
      let t4 = Ma(e2), n2 = (t5) => {
        if (e2.disposed) throw Xe2(t5);
        k2(e2.device, t5);
      };
      return { get time() {
        return n2(`clock.time`), t4.time;
      }, get deltaTime() {
        return n2(`clock.deltaTime`), t4.deltaTime;
      }, get frameCount() {
        return n2(`clock.frameCount`), t4.frameCount;
      }, advance(e3) {
        if (n2(`clock.advance`), typeof e3 != `number` || !Number.isFinite(e3) || e3 < 0) throw ot(e3);
        t4.advanceBy(e3);
      } };
    });
  }
  function Ms(e, t4, n2 = {}) {
    return Fs(A2(e, `frameLoop`)).loop(t4, n2);
  }
  var Ns;
  var Ps = gt(`frame-runner`);
  function Fs(e) {
    return e.service(Ps, (e2) => {
      let t4 = Ma(e2);
      return new Ks(() => {
        let t5 = () => {
        }, n2 = new Is(e2.device, void 0, (t6) => e2.reportError(t6), (t6) => {
          e2.trackDelivery(t6);
        }, () => t5());
        return t5 = e2.own(`scheduler`, () => n2.cancel()), n2;
      }, () => t4.tick(), (t5) => e2.own(`scheduler`, () => t5.stop()));
    });
  }
  var _e6, _t6, _n5, _r5, _i5, _a11, _o3, _Is_instances, s_fn4, c_fn4, l_fn4, u_fn2, d_fn2, f_fn, p_fn, _a12;
  var Is = (_a12 = class {
    constructor(e, t4, n2, r2, i2) {
      __privateAdd(this, _Is_instances);
      __publicField(this, "device");
      __publicField(this, "defaultTarget");
      __publicField(this, "errorSink");
      __publicField(this, "trackSettled");
      __publicField(this, "releaseLifecycle");
      __publicField(this, "done", Promise.resolve());
      __privateAdd(this, _e6);
      __privateAdd(this, _t6, []);
      __privateAdd(this, _n5, /* @__PURE__ */ new Set());
      __privateAdd(this, _r5, /* @__PURE__ */ new Set());
      __privateAdd(this, _i5, false);
      __privateAdd(this, _a11, false);
      __privateAdd(this, _o3, false);
      this.device = e, this.defaultTarget = t4, this.errorSink = n2, this.trackSettled = r2, this.releaseLifecycle = i2, k2(e, `Frame.constructor`), __privateSet(this, _e6, e.gpu.createCommandEncoder({ label: `vgpu.frame` }));
    }
    pass(e, t4) {
      if (__privateGet(this, _a11)) throw ct(`Frame.pass`);
      k2(this.device, `Frame.pass`);
      let n2 = ia(e), r2 = typeof t4 == `function` ? t4 : (e2) => e2.draw(t4), i2 = n2 ? e : e.target ?? this.defaultTarget;
      if (!i2) throw We(`Frame.pass`);
      if (Wa(i2) && __privateGet(this, _i5)) throw $e(`Frame.pass`);
      let a2 = n2 ? void 0 : e.clear, o2 = a2 === false;
      if (o2 && i2.sampleCount === 4) throw je();
      let c2 = n2 ? void 0 : e.clearDepth;
      if (c2 !== void 0) {
        if (typeof c2 != `number` || !(c2 >= 0 && c2 <= 1)) throw Me(c2);
        if (o2) throw Fe();
        if (!i2.depth) throw Me(c2, `but the target has no depth attachment, so clearDepth would have no effect.`, `Create the target with depth: true (or a depth format), or drop clearDepth.`);
      }
      let l2 = n2 ? void 0 : e.clearStencil;
      if (l2 !== void 0) {
        if (typeof l2 != `number` || !Number.isInteger(l2) || l2 < 0 || l2 > 4294967295) throw Ie(`received ${String(l2)}; expected an integer in [0, 0xFFFFFFFF] (WebGPU GPUStencilValue).`);
        if (o2) throw Le();
        let e2 = i2.depth?.format;
        if (!ta(e2)) throw Ie(`received ${String(l2)}, but the target's depth format ${e2 ? `"${e2}"` : `(none)`} has no stencil aspect, so clearStencil would have no effect.`);
      }
      let d2 = n2 ? void 0 : e.depthReadOnly;
      if (d2 !== void 0 && typeof d2 != `boolean`) throw Re(`received ${Z2(d2)}; expected a boolean.`, `Pass depthReadOnly: true to open the pass with a read-only depth attachment, or omit it.`);
      if (d2) {
        if (!i2.depth) throw Re(`is set, but the target has no depth attachment, so there is nothing to make read-only.`, `Create the target with depth: true (or a depth format), or drop depthReadOnly.`);
        if (i2.sampleCount === 4) throw ze();
        if (c2 !== void 0) throw Re(`cannot be combined with clearDepth; a read-only depth aspect omits its load/store ops and is never cleared.`, `Remove clearDepth, or drop depthReadOnly.`);
        if (l2 !== void 0) throw Re(`cannot be combined with clearStencil; a read-only stencil aspect omits its load/store ops and is never cleared.`, `Remove clearStencil, or drop depthReadOnly.`);
      }
      let f2 = n2 ? void 0 : Us(e.viewport, this.device.gpu.limits, i2.size), m2 = n2 ? void 0 : Ws(e.scissor, i2.size), g2 = [], x2;
      try {
        let t5 = n2 || e.timer === void 0 ? void 0 : __privateMethod(this, _Is_instances, d_fn2).call(this, e.timer, i2, g2, Vs), s3 = (n2 || e.visibility === void 0 ? void 0 : __privateMethod(this, _Is_instances, d_fn2).call(this, e.visibility, i2, g2, Hs))?.occlusion, u3 = i2.renderPassDescriptor({ clear: a2 === void 0 || a2 === true || a2 === false ? i2.clearColor ?? Gi : a2, preserve: o2, clearDepth: c2, clearStencil: l2, depthReadOnly: d2 });
        t5?.timestampWrites && (u3 = { ...u3, timestampWrites: t5.timestampWrites }), s3 && (u3 = { ...u3, occlusionQuerySet: s3.querySet }), x2 = __privateGet(this, _e6).beginRenderPass(u3), f2 && x2.setViewport(f2.x, f2.y, f2.width, f2.height, f2.minDepth, f2.maxDepth), m2 && x2.setScissorRect(m2[0], m2[1], m2[2], m2[3]), __privateSet(this, _o3, true);
        try {
          r2(new Ls(x2, i2, __privateGet(this, _t6), d2 === true, s3, this, (e2) => {
            if (k2(this.device, e2), __privateGet(this, _a11)) throw ct(e2);
          }));
        } finally {
          __privateSet(this, _o3, false);
        }
      } catch (e2) {
        __privateMethod(this, _Is_instances, l_fn4).call(this, g2), U2(__privateGet(this, _t6)), __privateGet(this, _t6).length = 0, _r2(this.device);
        try {
          x2?.end();
        } catch {
        }
        throw e2;
      }
      Tr(this.device, x2, __privateGet(this, _t6));
    }
    submit() {
      if (__privateGet(this, _i5) || __privateGet(this, _a11)) return;
      k2(this.device, `Frame.submit`), __privateSet(this, _i5, true), this.releaseLifecycle?.();
      for (let e2 of __privateMethod(this, _Is_instances, u_fn2).call(this)) e2.finalizeFrame(this, __privateGet(this, _e6));
      let e, t4 = __privateGet(this, _t6)[0]?.context;
      t4 && mr(this.device, t4);
      try {
        e = __privateGet(this, _e6).finish();
      } catch (e2) {
        __privateMethod(this, _Is_instances, s_fn4).call(this, __privateMethod(this, _Is_instances, c_fn4).call(this));
        let n3 = t4 ? H2(this.device) : void 0;
        U2(__privateGet(this, _t6)), n3 && U2([n3]);
        let r2 = n3?.context ?? t4;
        if (!r2) throw e2;
        this.done = __privateMethod(this, _Is_instances, p_fn).call(this, __privateMethod(this, _Is_instances, f_fn).call(this, r2.label, r2.group, e2));
        return;
      }
      if (t4) {
        let e2 = H2(this.device);
        e2 && (__privateGet(this, _t6)[0] = __privateGet(this, _t6)[0] ? br(e2, __privateGet(this, _t6)[0]) : e2);
      }
      let n2 = __privateGet(this, _t6)[0]?.context;
      n2 && mr(this.device, n2);
      try {
        this.device.gpu.queue.submit([e]);
      } catch (e2) {
        __privateMethod(this, _Is_instances, s_fn4).call(this, __privateMethod(this, _Is_instances, c_fn4).call(this));
        let t5 = n2 ? H2(this.device) : void 0;
        U2(__privateGet(this, _t6)), t5 && U2([t5]);
        let r2 = t5?.context ?? n2;
        if (!r2) throw e2;
        this.done = __privateMethod(this, _Is_instances, p_fn).call(this, __privateMethod(this, _Is_instances, f_fn).call(this, r2.label, r2.group, e2));
        return;
      }
      if (n2) {
        let e2 = H2(this.device);
        e2 && (__privateGet(this, _t6)[0] = __privateGet(this, _t6)[0] ? br(e2, __privateGet(this, _t6)[0]) : e2);
      }
      for (let e2 of __privateMethod(this, _Is_instances, u_fn2).call(this)) e2.frameSubmitted(this);
      __privateMethod(this, _Is_instances, s_fn4).call(this, __privateGet(this, _r5)), this.done = __privateMethod(this, _Is_instances, p_fn).call(this, yr(this.device, __privateGet(this, _t6), { errorSink: this.errorSink }));
    }
    cancel() {
      if (!__privateGet(this, _a11)) {
        if (__privateGet(this, _i5)) throw ut(`Frame.cancel`);
        if (__privateGet(this, _o3)) throw lt(`Frame.cancel`);
        __privateSet(this, _a11, true), this.releaseLifecycle?.(), __privateMethod(this, _Is_instances, s_fn4).call(this, __privateMethod(this, _Is_instances, c_fn4).call(this)), __privateGet(this, _n5).clear(), __privateGet(this, _r5).clear(), U2(__privateGet(this, _t6)), __privateGet(this, _t6).length = 0;
      }
    }
  }, _e6 = new WeakMap(), _t6 = new WeakMap(), _n5 = new WeakMap(), _r5 = new WeakMap(), _i5 = new WeakMap(), _a11 = new WeakMap(), _o3 = new WeakMap(), _Is_instances = new WeakSet(), s_fn4 = function(e) {
    for (let t4 of [...e]) t4.frameAbandoned(this);
  }, c_fn4 = function() {
    return [...__privateGet(this, _n5), ...__privateGet(this, _r5)];
  }, l_fn4 = function(e) {
    for (let t4 of [...e]) __privateGet(this, _n5).delete(t4), __privateGet(this, _r5).add(t4);
  }, u_fn2 = function() {
    return [...__privateGet(this, _n5)].filter((e) => !__privateGet(this, _r5).has(e));
  }, d_fn2 = function(e, t4, n2, r2) {
    let i2 = uo(e);
    if (!i2) throw r2(e);
    let a2;
    try {
      a2 = i2[lo]({ frame: this, device: this.device, target: t4 });
    } catch (e2) {
      throw __privateMethod(this, _Is_instances, l_fn4).call(this, __privateGet(this, _n5)), e2;
    }
    return __privateGet(this, _n5).add(a2.owner), n2.push(a2.owner), a2;
  }, f_fn = async function(e, t4, n2) {
    await vr(this.device), k2(this.device, `Frame.validation`);
    let r2 = _e(e, t4, n2);
    this.errorSink ? await this.errorSink(r2) : console.error(r2);
  }, p_fn = function(e) {
    return this.trackSettled?.(e), e;
  }, Ns = (e) => !__privateIn(_i5, e) || !__privateGet(e, _i5) && !__privateGet(e, _a11), _a12);
  var _e7, _a13;
  var Ls = (_a13 = class {
    constructor(e, t4, n2, r2 = false, i2, a2, o2) {
      __publicField(this, "encoder");
      __publicField(this, "target");
      __publicField(this, "validations");
      __publicField(this, "depthReadOnly");
      __publicField(this, "occlusionSource");
      __publicField(this, "frame");
      __publicField(this, "assertFrameOpen");
      __privateAdd(this, _e7, false);
      this.encoder = e, this.target = t4, this.validations = n2, this.depthReadOnly = r2, this.occlusionSource = i2, this.frame = a2, this.assertFrameOpen = o2;
    }
    draw(e, t4 = {}) {
      this.assertFrameOpen?.(`FramePass.draw`);
      let n2 = zs(e);
      this.depthReadOnly && Rs(n2, this.target), n2.encode(this.encoder, this.target, t4, (e2) => this.validations.push(e2));
    }
    occlusion(e, t4) {
      if (this.assertFrameOpen?.(`FramePass.occlusion`), !this.occlusionSource) throw He();
      if (__privateGet(this, _e7)) throw Ue();
      let n2 = this.occlusionSource.beginQuery(e, this.frame);
      this.encoder.beginOcclusionQuery(n2), __privateSet(this, _e7, true);
      try {
        typeof t4 == `function` ? t4() : this.draw(t4);
      } finally {
        __privateSet(this, _e7, false), this.encoder.endOcclusionQuery();
      }
    }
    bundles(...e) {
      if (this.assertFrameOpen?.(`FramePass.bundles`), this.depthReadOnly) throw Re(`pass cannot replay bundles: bundle records bundles with writable depth/stencil, and WebGPU only executes read-only-recorded bundles in a read-only pass.`, `Encode the draws directly with pass.draw(...) inside the depthReadOnly pass.`, `FramePass.bundles`);
      let t4 = e.map((e2) => co(e2) ?? Bs());
      for (let e2 of t4) e2.assertReplayable(this.target);
      this.encoder.executeBundles(t4.map((e2) => e2.gpu));
    }
  }, _e7 = new WeakMap(), _a13);
  function Rs(e, t4) {
    if (e.writesDepth()) throw Re(`pass cannot encode draw '${e.label}': its depth state writes depth (the default is write: true). Give the draw depth: { write: false } (or depth: false to disable depth testing).`, `Use depth: { write: false } on the draw, or open the pass without depthReadOnly.`, `FramePass.draw`);
    if (ta(t4.depth?.format)) {
      let t5 = e.stencilWritingOps();
      if (t5.length) throw Re(`pass cannot encode draw '${e.label}': its stencil ops can write (${t5.join(`, `)}), and the pass's stencil aspect is read-only too.`, `Use "keep" for those ops or stencil writeMask: 0, or open the pass without depthReadOnly.`, `FramePass.draw`);
    }
  }
  function zs(e) {
    let t4 = oo(e);
    if (!t4) throw TypeError(`Invalid Effect instance: pass.draw() expects a Draw or an Effect created by this library.`);
    return t4;
  }
  function Bs() {
    throw new Y({ code: `VGPU-R3-BUNDLE-INVALID`, message: `p.bundles() expected bundles created by bundle(gpu, { target }, cb).`, where: `FramePass.bundles` });
  }
  function Vs(e) {
    return Be(`FramePassOptions.timer received ${Z2(e)}; expected a TimerSpan from timer.span(name).`, `Create const passTimer = timer(gpu) once, then pass passTimer.span("name") per pass.`, `Frame.pass`);
  }
  function Hs(e) {
    return Ve(`FramePassOptions.visibility received ${Z2(e)}; expected a Visibility from visibility(gpu).`, `Create const vis = visibility(gpu) once, then pass { target, visibility: vis } per pass.`, `Frame.pass`);
  }
  function Us(e, t4, n2) {
    if (e === void 0) return;
    if (typeof e != `object` || !e || Array.isArray(e)) throw Ne(`received ${Z2(e)}; expected { x?, y?, width, height, minDepth?, maxDepth? }.`);
    let { x: r2 = 0, y: i2 = 0, width: a2, height: o2, minDepth: s3 = 0, maxDepth: c2 = 1 } = e;
    for (let [e2, t5] of [[`x`, r2], [`y`, i2], [`width`, a2], [`height`, o2], [`minDepth`, s3], [`maxDepth`, c2]]) if (typeof t5 != `number` || !Number.isFinite(t5)) throw Ne(`${e2} received ${Z2(t5)}; expected a finite number.`);
    let l2 = t4.maxTextureDimension2D, u3 = l2 * 2, d2 = `target is ${n2[0]}x${n2[1]}px, device maxTextureDimension2D is ${l2}`;
    if (!(a2 >= 0 && a2 <= l2)) throw Ne(`width ${a2} is outside [0, ${l2}] (${d2}).`);
    if (!(o2 >= 0 && o2 <= l2)) throw Ne(`height ${o2} is outside [0, ${l2}] (${d2}).`);
    if (!(r2 >= -u3 && r2 + a2 <= u3 - 1)) throw Ne(`x ${r2} with width ${a2} is outside [${-u3}, ${u3 - 1}] (${d2}).`);
    if (!(i2 >= -u3 && i2 + o2 <= u3 - 1)) throw Ne(`y ${i2} with height ${o2} is outside [${-u3}, ${u3 - 1}] (${d2}).`);
    if (!(s3 >= 0 && s3 <= 1)) throw Ne(`minDepth ${s3} is outside [0, 1].`);
    if (!(c2 >= 0 && c2 <= 1)) throw Ne(`maxDepth ${c2} is outside [0, 1].`);
    if (!(s3 <= c2)) throw Ne(`minDepth ${s3} exceeds maxDepth ${c2}.`);
    return { x: r2, y: i2, width: a2, height: o2, minDepth: s3, maxDepth: c2 };
  }
  function Ws(e, t4) {
    if (e === void 0) return;
    if (!Array.isArray(e) || e.length !== 4) throw Pe(`received ${Z2(e)}; expected [x, y, width, height].`);
    let [n2, i2, a2, o2] = e;
    for (let [e2, t5] of [[`x`, n2], [`y`, i2], [`width`, a2], [`height`, o2]]) if (typeof t5 != `number` || !Number.isInteger(t5) || t5 < 0) throw Pe(`${e2} received ${Z2(t5)}; expected a non-negative integer.`);
    let [s3, c2] = t4;
    if (n2 + a2 > s3 || i2 + o2 > c2) throw Pe(`[${n2}, ${i2}, ${a2}, ${o2}] exceeds the target's current size ${s3}x${c2}px (x + width <= ${s3}, y + height <= ${c2}).`);
    return [n2, i2, a2, o2];
  }
  function Z2(e) {
    return typeof e == `string` ? `'${e}'` : Array.isArray(e) ? `[${e.map((e2) => Z2(e2)).join(`, `)}]` : typeof e == `object` && e ? `an object` : String(e);
  }
  function Gs(e) {
    let t4 = e?.code;
    return t4 === `VGPU-DEVICE-DISPOSED` || t4 === `VGPU-DEVICE-LOST`;
  }
  var _e8, _a14;
  var Ks = (_a14 = class {
    constructor(e, t4, n2) {
      __publicField(this, "createFrame");
      __publicField(this, "advance");
      __publicField(this, "trackLoop");
      __privateAdd(this, _e8, false);
      this.createFrame = e, this.advance = t4, this.trackLoop = n2;
    }
    frame(e) {
      if (__privateGet(this, _e8) || Ba()) throw st();
      __privateSet(this, _e8, true), Ha();
      try {
        this.advance();
        let t4 = this.createFrame();
        if (e) {
          try {
            e(t4);
          } catch (e2) {
            if (Ns(t4)) try {
              t4.cancel();
            } catch {
            }
            throw e2;
          }
          try {
            t4.submit();
          } catch (e2) {
            if (!Gs(e2)) throw e2;
          }
        }
        return t4;
      } finally {
        Ua(), __privateSet(this, _e8, false);
      }
    }
    loop(e, t4 = {}) {
      let n2 = false, r2 = globalThis.requestAnimationFrame ?? ((e2) => setTimeout(() => e2(performance.now()), 16)), i2 = globalThis.cancelAnimationFrame ?? ((e2) => clearTimeout(e2)), a2 = t4.fps && t4.fps > 0 ? 1e3 / t4.fps : 0, o2, s3 = 0, c2, l2 = () => {
        n2 = true, i2(s3), c2?.(), c2 = void 0;
      }, u3 = (t5) => {
        if (!n2) {
          if (Js(t5, o2, a2)) {
            o2 = t5;
            try {
              this.frame(e);
            } catch (e2) {
              throw l2(), e2;
            }
          }
          n2 || (s3 = r2(u3));
        }
      };
      s3 = r2(u3);
      let d2 = { stop: l2 };
      return c2 = this.trackLoop?.(d2), d2;
    }
  }, _e8 = new WeakMap(), _a14);
  var qs = 1;
  function Js(e, t4, n2) {
    return t4 === void 0 || n2 <= 0 || e - t4 >= n2 - qs;
  }
  function Ys(e) {
    let t4 = [];
    for (let n2 of e) try {
      n2.destroy();
    } catch (e2) {
      t4.push(e2);
    }
    if (t4.length) throw t4[0];
  }
  function Xs(e, t4) {
    return new Zs(A2(e, `target`).device, t4);
  }
  var _e9, _t7, _n6, _r6, _i6, _a15, _o4, _Zs_instances, s_fn5, c_fn5, l_fn5, u_fn3, d_fn3, f_fn2, _a16;
  var Zs = (_a16 = class {
    constructor(e, t4) {
      __privateAdd(this, _Zs_instances);
      __publicField(this, "device");
      __publicField(this, "resourceIdentity", h(`render-target`));
      __privateAdd(this, _e9, new g());
      __privateAdd(this, _t7, /* @__PURE__ */ new Set());
      __privateAdd(this, _n6);
      __privateAdd(this, _r6);
      __privateAdd(this, _i6);
      __privateAdd(this, _a15, false);
      __privateAdd(this, _o4, false);
      __publicField(this, "options");
      this.device = e, Zi(t4, e), this.options = Object.freeze({ ...t4, size: Object.freeze([...t4.size]), ...t4.colors ? { colors: Object.freeze(t4.colors.map((e2) => Object.freeze({ ...e2 }))) } : {} }), __privateSet(this, _i6, t4.clearColor === void 0 ? Gi : Ki(t4.clearColor, `target.clearColor`)), __privateSet(this, _n6, this.options.size), __privateSet(this, _r6, __privateMethod(this, _Zs_instances, l_fn5).call(this, __privateGet(this, _n6)));
    }
    get gpu() {
      return this.color.gpu;
    }
    get size() {
      return __privateGet(this, _n6);
    }
    get texelSize() {
      return [1 / __privateGet(this, _n6)[0], 1 / __privateGet(this, _n6)[1]];
    }
    get color() {
      return __privateGet(this, _r6).colors[0];
    }
    get colors() {
      return __privateGet(this, _r6).colors;
    }
    get depth() {
      return __privateGet(this, _r6).depth;
    }
    get format() {
      return Ji(this.options)[0]?.format ?? `rgba8unorm`;
    }
    get clearColor() {
      return qi(__privateGet(this, _i6));
    }
    set clearColor(e) {
      __privateSet(this, _i6, Ki(e, `target.clearColor`));
    }
    get sampleCount() {
      return Xi(this.options);
    }
    resize(e) {
      if (__privateMethod(this, _Zs_instances, s_fn5).call(this, `Target.resize`), __privateGet(this, _o4)) throw new Y({ code: `VGPU-TARGET-RESIZE-REENTRANT`, message: `Cannot resize a target from its replacement callbacks.`, where: `Target.resize` });
      if (Zi({ ...this.options, size: e }, this.device), !ra(__privateGet(this, _n6), e)) {
        __privateSet(this, _o4, true);
        try {
          let t4 = Object.freeze([...e]), n2 = __privateMethod(this, _Zs_instances, l_fn5).call(this, t4), r2 = __privateGet(this, _r6);
          __privateSet(this, _n6, t4), __privateSet(this, _r6, n2);
          try {
            __privateMethod(this, _Zs_instances, c_fn5).call(this);
          } finally {
            Ys(r2.all);
          }
        } finally {
          __privateSet(this, _o4, false);
        }
      }
    }
    onDestroy(e) {
      return __privateGet(this, _e9).onDestroy(this, e);
    }
    onTexturesRecreated(e) {
      return __privateMethod(this, _Zs_instances, s_fn5).call(this, `Target.onTexturesRecreated`), __privateGet(this, _t7).add(e), () => {
        __privateGet(this, _t7).delete(e);
      };
    }
    destroy() {
      if (!__privateGet(this, _a15)) {
        __privateSet(this, _a15, true);
        try {
          __privateGet(this, _e9).emit(this);
        } finally {
          __privateGet(this, _t7).clear(), Ys(__privateGet(this, _r6).all);
        }
      }
    }
    renderPassDescriptor(e = {}) {
      __privateMethod(this, _Zs_instances, s_fn5).call(this, `Target.renderPassDescriptor`);
      let { clear: t4 = [0, 0, 0, 1], preserve: n2, clearDepth: r2, clearStencil: i2, depthReadOnly: a2 } = e;
      return { colorAttachments: this.colors.map((e2, r3) => $i(e2, __privateGet(this, _r6).msaaColors?.[r3], t4, n2)), depthStencilAttachment: this.depth ? ea(this.depth, n2, r2, i2, a2) : void 0 };
    }
  }, _e9 = new WeakMap(), _t7 = new WeakMap(), _n6 = new WeakMap(), _r6 = new WeakMap(), _i6 = new WeakMap(), _a15 = new WeakMap(), _o4 = new WeakMap(), _Zs_instances = new WeakSet(), s_fn5 = function(e) {
    if (__privateGet(this, _a15)) throw new Y({ code: `VGPU-TARGET-DESTROYED`, message: `Target '${this.options.label ?? `target`}' is destroyed.`, where: e, fix: `Create a new target; destroyed targets cannot be resized or rendered.` });
    k2(this.device, e);
  }, c_fn5 = function() {
    let e = [];
    for (let t4 of [...__privateGet(this, _t7)]) try {
      t4();
    } catch (t5) {
      e.push(t5);
    }
    if (e.length) throw e[0];
  }, l_fn5 = function(e) {
    let t4 = [], n2 = (e2) => {
      let n3 = this.device.createTexture(e2);
      return t4.push(n3), n3;
    };
    try {
      let r2 = __privateMethod(this, _Zs_instances, u_fn3).call(this, e, n2), i2 = this.sampleCount === 4 ? __privateMethod(this, _Zs_instances, d_fn3).call(this, e, n2) : void 0, a2 = __privateMethod(this, _Zs_instances, f_fn2).call(this, e, n2);
      return { colors: Object.freeze(r2), msaaColors: i2 && Object.freeze(i2), depth: a2, all: t4 };
    } catch (e2) {
      try {
        Ys(t4);
      } catch {
      }
      throw e2;
    }
  }, u_fn3 = function(e, t4) {
    return Ji(this.options).map((n2, r2) => t4({ kind: `2d`, size: e, format: n2.format, usage: [`render_attachment`, `texture_binding`, `copy_src`], sampleCount: 1, label: this.options.label ? `${this.options.label}.color${r2}.resolve` : void 0 }));
  }, d_fn3 = function(e, t4) {
    return Ji(this.options).map((n2, r2) => t4({ kind: `2d`, size: e, format: n2.format, usage: [`render_attachment`], sampleCount: 4, label: this.options.label ? `${this.options.label}.color${r2}` : void 0 }));
  }, f_fn2 = function(e, t4) {
    let n2 = Yi(this.options);
    return n2 ? t4({ kind: `2d`, size: e, format: n2, usage: [`render_attachment`, `texture_binding`], sampleCount: this.sampleCount, label: this.options.label ? `${this.options.label}.depth` : void 0 }) : void 0;
  }, _a16);
  var Qs = `

const KERNEL_RADIUS: u32 = 22u;
struct BlurUniforms {
  direction: vec2f,
  invSize: vec2f,
  gaussianCoefficients0: vec4f,
  gaussianCoefficients1: vec4f,
  gaussianCoefficients2: vec4f,
  gaussianCoefficients3: vec4f,
  gaussianCoefficients4: vec4f,
  gaussianCoefficients5: vec4f,
};
@group(0) @binding(0) var<uniform> uniforms: BlurUniforms;
@group(0) @binding(1) var colorTexture: texture_2d<f32>;
@group(0) @binding(2) var linearSampler: sampler;

fn coefficient(i: u32) -> f32 {
  let packed = array<vec4f, 6>(
    uniforms.gaussianCoefficients0,
    uniforms.gaussianCoefficients1,
    uniforms.gaussianCoefficients2,
    uniforms.gaussianCoefficients3,
    uniforms.gaussianCoefficients4,
    uniforms.gaussianCoefficients5
  );
  return packed[i / 4u][i % 4u];
}

@fragment fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {

  var weightSum = coefficient(0u);
  var diffuseSum = textureSample(colorTexture, linearSampler, uv).rgb * weightSum;
  for (var i = 1u; i < KERNEL_RADIUS; i = i + 1u) {
    let x = f32(i);
    let w = coefficient(i);
    let uvOffset = uniforms.direction * uniforms.invSize * x;
    let sample1 = textureSample(colorTexture, linearSampler, uv + uvOffset).rgb;
    let sample2 = textureSample(colorTexture, linearSampler, uv - uvOffset).rgb;
    diffuseSum = diffuseSum + (sample1 + sample2) * w;
  }
  return vec4f(diffuseSum, 1.0);
}
`;
  var $s = `struct BrightUniforms {
  luminosityThreshold: f32,
  smoothWidth: f32,
  _pad0: vec2f,
};
@group(0) @binding(0) var<uniform> uniforms: BrightUniforms;
@group(0) @binding(1) var tDiffuse: texture_2d<f32>;
@group(0) @binding(2) var linearSampler: sampler;

fn luminance(rgb: vec3f) -> f32 {
  return dot(rgb, vec3f(0.299, 0.587, 0.114));
}

@fragment fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {

  let texel = textureSample(tDiffuse, linearSampler, uv);
  let v = luminance(texel.xyz);
  let outputColor = vec4f(vec3f(0.0), 0.0);
  let alpha = smoothstep(uniforms.luminosityThreshold, uniforms.luminosityThreshold + uniforms.smoothWidth, v);
  return mix(outputColor, texel, alpha);
}
`;
  var ec = `struct CompositeUniforms {
  bloomStrength: f32,
  bloomRadius: f32,
  _pad0: vec2f,
  bloomFactors0: vec4f,
  bloomFactors1: vec4f,
};
@group(0) @binding(0) var<uniform> uniforms: CompositeUniforms;
@group(0) @binding(1) var blurTexture1: texture_2d<f32>;
@group(0) @binding(2) var blurTexture2: texture_2d<f32>;
@group(0) @binding(3) var blurTexture3: texture_2d<f32>;
@group(0) @binding(4) var blurTexture4: texture_2d<f32>;
@group(0) @binding(5) var blurTexture5: texture_2d<f32>;
@group(0) @binding(6) var linearSampler: sampler;

fn factor(i: u32) -> f32 {
  let v = array<f32, 8>(
    uniforms.bloomFactors0.x, uniforms.bloomFactors0.y, uniforms.bloomFactors0.z, uniforms.bloomFactors0.w,
    uniforms.bloomFactors1.x, uniforms.bloomFactors1.y, uniforms.bloomFactors1.z, uniforms.bloomFactors1.w
  );
  return v[i];
}

fn lerpBloomFactor(f: f32) -> f32 {
  let mirrorFactor = 1.2 - f;
  return mix(f, mirrorFactor, uniforms.bloomRadius);
}

@fragment fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {

  let bloom = 3.0 * uniforms.bloomStrength * (
    lerpBloomFactor(factor(0u)) * textureSample(blurTexture1, linearSampler, uv).rgb +
    lerpBloomFactor(factor(1u)) * textureSample(blurTexture2, linearSampler, uv).rgb +
    lerpBloomFactor(factor(2u)) * textureSample(blurTexture3, linearSampler, uv).rgb +
    lerpBloomFactor(factor(3u)) * textureSample(blurTexture4, linearSampler, uv).rgb +
    lerpBloomFactor(factor(4u)) * textureSample(blurTexture5, linearSampler, uv).rgb
  );
  let bloomAlpha = max(bloom.r, max(bloom.g, bloom.b));
  return vec4f(bloom, bloomAlpha);
}
`;
  var tc = { simulation: { oceanSize: 200, worldSize: 400, timeScale: 0.6, spectrumTimeScale: 0.5, windSpeed: 12.9, windAngle: 4.83, amplitude: 1.3, choppiness: 1.51, displacementScale: 5e-3, foamThreshold: 0 }, particles: { pointSize: 0.75, fadeNear: 60, fadeFar: 250, fadePower: 3.2, oceanColor: [0.003035269835488375, 0.003035269835488375, 0.003035269835488375, 0], neonColor: [1, 1, 1, 0], foamColor: [1, 1, 1, 0] }, camera: { eye: [0, 30, 90], target: [0, 5, 55], pitchDegrees: -10, fovDegrees: 90, near: 0.1, far: 2e3 }, present: { fgColor: [0.682, 0.678, 0.671], bgColor: [0.039, 0.039, 0.039], brightness: 2.1 }, bloom: { threshold: 0.3, smoothWidth: 0.01, strength: 0.08, radius: 0.46, levels: 5, kernelRadii: [6, 10, 14, 18, 22] }, bottomFadeStartPercent: 100 };
  var nc = { simulation: { worldSize: 700, displacementScale: 0.035 }, particles: { pointSize: 1.1, fadeFar: 520, fadePower: 1.8 }, camera: { eye: [0, 14, 78], target: [0, 0, -60], pitchDegrees: 21, fovDegrees: 95 }, bottomFadeStartPercent: 62 };
  var Q2 = { simulation: { ...tc.simulation, ...nc.simulation }, particles: { ...tc.particles, ...nc.particles }, camera: { ...tc.camera, ...nc.camera }, present: { ...tc.present, ...nc.present }, bloom: { ...tc.bloom, ...nc.bloom }, bottomFadeStartPercent: nc.bottomFadeStartPercent ?? tc.bottomFadeStartPercent };
  function rc(e) {
    return Array.from({ length: 24 }, (t4, n2) => n2 < e ? 0.39894 * Math.exp(-0.5 * n2 * n2 / (e / 3) ** 2) / (e / 3) : 0);
  }
  function ic(e) {
    let { eye: t4, target: n2, pitchDegrees: r2, fovDegrees: i2, near: a2, far: o2 } = Q2.camera, s3 = Math.atan2(t4[1] - n2[1], t4[2] - n2[2]) - r2 * Math.PI / 180, c2 = Math.cos(s3), l2 = Math.sin(s3), u3 = 1 / Math.tan(i2 * Math.PI / 360), d2 = new Float32Array(16);
    d2[0] = d2[15] = 1, d2[5] = d2[10] = c2, d2[6] = l2, d2[9] = -l2, d2[13] = -(c2 * t4[1] - l2 * t4[2]), d2[14] = -(l2 * t4[1] + c2 * t4[2]);
    let f2 = new Float32Array(16);
    return f2[0] = u3 / (e[0] / Math.max(1, e[1])), f2[5] = u3, f2[10] = o2 / (a2 - o2), f2[11] = -1, f2[14] = o2 * a2 / (a2 - o2), { view: d2, projection: f2 };
  }
  var ac = `struct IfftStageUniforms {
  resolution: f32,
  subtransformSize: f32,
  horizontal: f32,
};
@group(0) @binding(0) var<uniform> u: IfftStageUniforms;
@group(0) @binding(1) var u_input: texture_2d<f32>;

@fragment fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let N = i32(u.resolution);
  let horizontal = u.horizontal > 0.5;
  let index = select(position.y - 0.5, position.x - 0.5, horizontal);

  let evenIndex = floor(index / u.subtransformSize) * (u.subtransformSize * 0.5)
                + (index % (u.subtransformSize * 0.5));

  let evenCoord = select(
    vec2i(i32(position.x - 0.5), i32(evenIndex)),
    vec2i(i32(evenIndex), i32(position.y - 0.5)),
    horizontal,
  );
  let oddCoord = select(
    vec2i(i32(position.x - 0.5), i32(evenIndex + u.resolution * 0.5)),
    vec2i(i32(evenIndex + u.resolution * 0.5), i32(position.y - 0.5)),
    horizontal,
  );
  let even = wrapLoad(u_input, evenCoord, N);
  let odd = wrapLoad(u_input, oddCoord, N);

  let twiddleArg = 2.0 * PI * (index / u.subtransformSize);
  let twiddle = vec2f(cos(twiddleArg), sin(twiddleArg));

  let outA = even.xy + cmul(twiddle, odd.xy);
  let outB = even.zw + cmul(twiddle, odd.zw);
  return vec4f(outA, outB);
}
`;
  var oc = `struct InitialSpectrumUniforms {
  resolution: f32,
  size: f32,
  windSpeed: f32,
  windAngle: f32,
  amplitude: f32,
};
@group(0) @binding(0) var<uniform> u: InitialSpectrumUniforms;
@group(0) @binding(1) var u_noise: texture_2d<f32>;

fn phillips(k: vec2f) -> f32 {
  let kk = dot(k, k);
  if (kk < 1e-8) { return 0.0; }
  let w = vec2f(cos(u.windAngle), sin(u.windAngle));
  let L = (u.windSpeed * u.windSpeed) / G;
  let kdotw = dot(normalize(k), w);
  var ph = u.amplitude * exp(-1.0 / (kk * L * L)) / (kk * kk) * (kdotw * kdotw);
  let l = L * 0.001;
  ph *= exp(-kk * l * l);
  if (kdotw < 0.0) { ph *= 0.07; }
  return ph;
}

@fragment fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let coord = position.xy - vec2f(0.5);
  let n = select(coord.x - u.resolution, coord.x, coord.x < u.resolution * 0.5);
  let m = select(coord.y - u.resolution, coord.y, coord.y < u.resolution * 0.5);
  let k = (2.0 * PI / u.size) * vec2f(n, m);

  let rnd = textureLoad(u_noise, vec2u(coord), 0);

  let h0k    = (1.0 / sqrt(2.0)) * vec2f(rnd.x, rnd.y) * sqrt(phillips(k));
  let h0negk = (1.0 / sqrt(2.0)) * vec2f(rnd.z, rnd.w) * sqrt(phillips(-k));

  return vec4f(h0k, h0negk.x, -h0negk.y);
}
`;
  var sc = `const SEED: u32 = 0x6f636561u;
const RESOLUTION: u32 = 512u;



fn mulberryAt(callIndex: u32) -> f32 {
  let state = SEED + 0x6d2b79f5u * (callIndex + 1u);
  var t = state;
  t = (t ^ (t >> 15u)) * (t | 1u);
  t = t ^ (t + ((t ^ (t >> 7u)) * (t | 61u)));
  return f32(t ^ (t >> 14u)) / 4294967296.0;
}
fn gaussianPair(callIndex: u32) -> vec2f {
  let u1 = max(mulberryAt(callIndex), 1.17549435e-38);
  let u2 = mulberryAt(callIndex + 1u);
  let magnitude = sqrt(-2.0 * log(u1));
  let angle = 6.283185307179586 * u2;
  return magnitude * vec2f(cos(angle), sin(angle));
}
@fragment fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let coord = vec2u(position.xy - vec2f(0.5));
  let base = (coord.y * RESOLUTION + coord.x) * 4u;
  return vec4f(gaussianPair(base), gaussianPair(base + 2u));
}
`;
  var cc = `struct NormalFoamUniforms {
  resolution: f32,
  worldSize: f32,
  displacementScale: f32,
  foamThreshold: f32,
};
@group(0) @binding(0) var<uniform> u: NormalFoamUniforms;
@group(0) @binding(1) var u_displacement: texture_2d<f32>;

@fragment fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let N = i32(u.resolution);
  let coord = vec2i(position.xy - vec2f(0.5));
  let dx = u.worldSize / u.resolution;

  let r = wrapLoad(u_displacement, coord + vec2i(1, 0), N).xyz * u.displacementScale;
  let l = wrapLoad(u_displacement, coord - vec2i(1, 0), N).xyz * u.displacementScale;
  let t = wrapLoad(u_displacement, coord + vec2i(0, 1), N).xyz * u.displacementScale;
  let b = wrapLoad(u_displacement, coord - vec2i(0, 1), N).xyz * u.displacementScale;

  let dhdx = (r.y - l.y) / (2.0 * dx);
  let dhdz = (t.y - b.y) / (2.0 * dx);
  let normal = normalize(vec3f(-dhdx, 1.0, -dhdz));

  let dDxdx = (r.x - l.x) / (2.0 * dx);
  let dDzdz = (t.z - b.z) / (2.0 * dx);
  let dDxdz = (t.x - b.x) / (2.0 * dx);
  let dDzdx = (r.z - l.z) / (2.0 * dx);
  let J = (1.0 + dDxdx) * (1.0 + dDzdz) - dDxdz * dDzdx;

  let foam = 1.0 - smoothstep(u.foamThreshold, u.foamThreshold + 0.8, J);
  return vec4f(normal, foam);
}
`;
  var lc = { fg: Q2.present.fgColor, bg: Q2.present.bgColor };
  var uc = `const PI: f32 = 3.141592653589793;
const G: f32 = 9.81;

fn cmul(a: vec2f, b: vec2f) -> vec2f {
  return vec2f(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

fn wrapCoord(coord: vec2i, N: i32) -> vec2u {
  let wrapped = (coord % vec2i(N) + vec2i(N)) % vec2i(N);
  return vec2u(wrapped);
}

fn wrapLoad(tex: texture_2d<f32>, coord: vec2i, N: i32) -> vec4f {
  return textureLoad(tex, wrapCoord(coord, N), 0);
}
`;
  var dc = 9;
  function fc() {
    return Object.freeze(Array.from({ length: dc * 2 }, (e, t4) => {
      let n2 = t4 % 2 ? `pong` : `ping`;
      return Object.freeze({ index: t4, axisStage: t4 % dc, horizontal: t4 < dc, subtransformSize: 2 ** (t4 % dc + 1), input: t4 === 0 ? `spectrum` : n2 === `ping` ? `pong` : `ping`, output: n2 });
    }));
  }
  var pc = `struct ParticleUniforms {
  view: mat4x4f,
  projection: mat4x4f,
  viewport: vec4f,
  world: vec4f,
  fade: vec4f,
  oceanColor: vec4f,
  neonColor: vec4f,
  foamColor: vec4f,
  cursor: vec4f,
};

struct VertexOut {
  @builtin(position) position: vec4f,
  @location(0) pointCoord: vec2f,
  @location(1) foam: f32,
  @location(2) normal: vec3f,
  @location(3) viewDir: vec3f,
  @location(4) height: f32,
  @location(5) fade: f32,
};

@group(0) @binding(0) var<uniform> u: ParticleUniforms;
@group(0) @binding(1) var u_displacement: texture_2d<f32>;
@group(0) @binding(2) var u_normalFoam: texture_2d<f32>;

fn particleHash(p: vec2u) -> f32 {
  var n = p.x * 374761393u + p.y * 668265263u;
  n = (n ^ (n >> 13u)) * 1274126177u;
  n = n ^ (n >> 16u);
  return f32(n) / 4294967295.0;
}

fn quadCorner(vertexIndex: u32) -> vec2f {
  let cornerIndex = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u)[vertexIndex % 6u];
  switch (cornerIndex) {
    case 0u: { return vec2f(-1.0, -1.0); }
    case 1u: { return vec2f( 1.0, -1.0); }
    case 2u: { return vec2f(-1.0,  1.0); }
    default: { return vec2f( 1.0,  1.0); }
  }
}

@vertex fn vs_main(
  @builtin(vertex_index) vertexIndex: u32,
  @builtin(instance_index) instanceIndex: u32,
) -> VertexOut {
  let resolution = max(1u, u32(u.viewport.w));
  let i = instanceIndex % resolution;
  let j = instanceIndex / resolution;
  let particleRef = vec2f(f32(i), f32(j)) / f32(resolution);
  let texCoord = vec2u(i, j);

  let disp = textureLoad(u_displacement, texCoord, 0).xyz * u.world.y;
  let nf = textureLoad(u_normalFoam, texCoord, 0);

  let halfWorld = u.world.x * 0.5;
  let base = vec3f(
    particleRef.x * u.world.x - halfWorld,
    0.0,
    particleRef.y * u.world.x - halfWorld,
  );
  let pos = base + disp;

  let mv = u.view * vec4f(pos, 1.0);
  let viewDir = -mv.xyz;
  let dist = -mv.z;
  let f = 1.0 - smoothstep(u.fade.x, u.fade.y, dist);
  let fade = pow(clamp(f, 0.0, 1.0), u.fade.z);

  let projected = u.projection * mv;
  var ndc = projected.xy / projected.w;

  let aspect = u.viewport.x / max(u.viewport.y, 1.0);
  if (u.cursor.z > 0.0) {
    let cursorDelta = u.cursor.xy - ndc;
    let fieldDelta = cursorDelta * vec2f(aspect, 1.0);
    let cursorDistance = length(fieldDelta);
    let rangeFalloff = 1.0 - smoothstep(0.0, 1.7, cursorDistance);
    let particleId = vec2u(i, j);
    let particleSeed = particleHash(particleId);
    let detached = smoothstep(0.76, 0.998, particleSeed);
    let phase = particleHash(particleId + vec2u(37u, 71u)) * 6.2831855;
    let activity = 0.55 + 0.45 * (0.5 + 0.5 * sin(u.cursor.w * 0.45 + phase));
    let trailProgress = clamp(cursorDistance / 1.7, 0.0, 1.0);
    let trailWidth = mix(0.025, 0.28, trailProgress);
    let jitter = vec2f(
      particleHash(particleId + vec2u(17u, 53u)) - 0.5,
      particleHash(particleId + vec2u(41u, 29u)) - 0.5,
    ) * vec2f(trailWidth / aspect, trailWidth);
    let nearFalloff = 1.0 - smoothstep(0.0, 1.0, cursorDistance);
    let baselineResponse = mix(0.004, 0.014, nearFalloff) *
      mix(0.6, 1.0, particleSeed);
    let particleResponse = baselineResponse +
      detached * activity * mix(0.11, 0.82, nearFalloff);
    let attraction = rangeFalloff * particleResponse * u.cursor.z;
    ndc += (u.cursor.xy + jitter - ndc) * attraction;
  }

  let corner = quadCorner(vertexIndex);
  let pointSizePx = 2.0 * u.world.z * u.viewport.z;
  let clipOffset = corner * (pointSizePx / u.viewport.xy) * projected.w;
  let clip = vec4f(ndc * projected.w + clipOffset, projected.z, projected.w);

  var out: VertexOut;
  out.position = clip;
  out.pointCoord = corner * 0.5 + vec2f(0.5);
  out.foam = nf.w;
  out.normal = nf.xyz;
  out.viewDir = viewDir;
  out.height = disp.y;
  out.fade = fade;
  return out;
}

@fragment fn fs_main(in: VertexOut) -> @location(0) vec4f {
  let cc = in.pointCoord - vec2f(0.5);
  let d2 = dot(cc, cc);
  if (d2 > 0.25) {
    discard;
  }

  let n = normalize(in.normal);
  let v = normalize(in.viewDir);
  let fresnel = pow(1.0 - clamp(dot(n, v), 0.0, 1.0), 5.0);

  let foam = clamp(in.foam, 0.0, 1.0);
  let crest = smoothstep(-0.5, 1.5, in.height);

  var color = u.oceanColor.rgb * 0.5;
  color += u.neonColor.rgb * crest * 0.5;
  color += u.neonColor.rgb * fresnel * 0.15;
  color = mix(color, u.foamColor.rgb, foam);
  var alpha = 0.02 + crest * 0.06 + fresnel * 0.04;
  alpha = mix(alpha, 1.0, foam);
  color *= in.fade;
  alpha *= in.fade;
  return vec4f(color, clamp(alpha, 0.0, 1.0));
}
`;
  var mc = `struct PresentUniforms {
  fgColor: vec4f,
  bgColor: vec4f,

  brightness: f32,
  _pad: vec3f,
};

@group(0) @binding(0) var<uniform> uniforms: PresentUniforms;
@group(0) @binding(1) var sceneHDR: texture_2d<f32>;
@group(0) @binding(2) var bloomTexture: texture_2d<f32>;
@group(0) @binding(3) var linearSampler: sampler;

fn LinearTosRGB(value: vec4f) -> vec4f {
  let lt = value.rgb * 12.92;
  let gt = 1.055 * pow(value.rgb, vec3f(0.41666)) - vec3f(0.055);
  let rgb = select(gt, lt, value.rgb <= vec3f(0.0031308));
  return vec4f(rgb, value.a);
}

@fragment fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {
  let scene = textureSample(sceneHDR, linearSampler, uv);
  let bloom = textureSample(bloomTexture, linearSampler, uv);
  let hdr = scene.rgb + bloom.rgb;




  let tone = clamp(dot(hdr, vec3f(0.2126, 0.7152, 0.0722)), 0.0, 1.0);





  var color = mix(uniforms.bgColor.rgb, uniforms.fgColor.rgb, tone);
  color += (uniforms.fgColor.rgb - uniforms.bgColor.rgb) * tone * tone
    * (uniforms.brightness - 1.0);

  return LinearTosRGB(vec4f(clamp(color, vec3f(0.0), vec3f(1.0)), 1.0));
}
`;
  var hc = `struct SpectrumUniforms {
  resolution: f32,
  size: f32,
  time: f32,
  choppiness: f32,
};
@group(0) @binding(0) var<uniform> u: SpectrumUniforms;
@group(0) @binding(1) var u_initialSpectrum: texture_2d<f32>;

@fragment fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let coord = position.xy - vec2f(0.5);
  let n = select(coord.x - u.resolution, coord.x, coord.x < u.resolution * 0.5);
  let m = select(coord.y - u.resolution, coord.y, coord.y < u.resolution * 0.5);
  let k = (2.0 * PI / u.size) * vec2f(n, m);
  let kLen = length(k);

  let h0 = textureLoad(u_initialSpectrum, vec2u(coord), 0);
  let w = sqrt(G * kLen) * u.time;
  let expp = vec2f(cos(w),  sin(w));
  let expm = vec2f(cos(w), -sin(w));

  let h = cmul(h0.rg, expp) + cmul(h0.ba, expm);



  var kn = vec2f(0.0);
  if (kLen > 0.0) { kn = k / kLen; }
  let negI_h = vec2f(h.y, -h.x);
  let hx = negI_h * kn.x * u.choppiness;
  let hz = negI_h * kn.y * u.choppiness;

  let cA = vec2f(hx.x - h.y, hx.y + h.x);
  let cB = hz;
  return vec4f(cA, cB);
}
`;
  var gc = `${uc}
${ac}`;
  var _c = `${uc}
${oc}`;
  var vc = `${uc}
${cc}`;
  var yc = `${uc}
${hc}`;
  var bc = 0.22;
  var xc = 0.16;
  var Sc = `rgba32float`;
  var Cc = `rgba16float`;
  var wc = [0, 0, 0, 0];
  function Tc({ canvas: t4, colors: n2, fps: r2, onError: i2 }) {
    let a2 = false, o2 = n2 ?? lc, s3 = [0, 0, 0], c2 = [0, 0, 0], l2, u3 = false, d2 = false, f2 = false, p2 = false, m2, h2 = () => {
    }, g2 = () => {
    }, _2 = new Promise((e, t5) => {
      h2 = e, g2 = t5;
    });
    _2.catch(() => {
    });
    let v2 = false, y2, b2, x2, ee2, S2 = 0, C2 = 0;
    function te2() {
      a2 || (a2 = true, C2++, Rc([() => {
        S2 && cancelAnimationFrame(S2);
      }, () => l2?.stop(), () => m2?.(), () => ee2?.(), () => y2?.dispose()]));
    }
    function w2(e) {
      let t5 = !d2;
      d2 = true;
      try {
        te2();
      } catch {
      }
      if (t5 && g2(e), !i2) throw e;
      t5 && i2(e);
    }
    let ne2 = async (e) => {
      if (a2 || !y2 || !b2 || !x2 || Bc(x2.scene.size, b2.size)) return;
      let t5 = await Ec(y2, b2, `fft-ocean-resize-${e}`, o2);
      if (a2) return;
      if (e !== C2) {
        try {
          Ic(t5);
        } catch {
        }
        return;
      }
      let n3 = x2;
      x2 = t5, Ic(n3);
    }, re2 = () => {
      if (a2) return;
      if (f2) {
        p2 = true;
        return;
      }
      if (S2) return;
      let e = ++C2;
      S2 = requestAnimationFrame(async () => {
        S2 = 0, f2 = true;
        try {
          await ne2(e);
        } catch (t5) {
          !a2 && e === C2 && w2(t5);
        } finally {
          f2 = false;
        }
        p2 && !a2 && (p2 = false, re2());
      });
    }, ie2 = (async () => {
      let { init: n3 } = await t(async () => {
        let { init: e } = await Promise.resolve().then(() => (init_dist_CW40cPHt(), dist_CW40cPHt_exports)).then((e2) => e2.t);
        return { init: e };
      }, __vite__mapDeps([0, 1]));
      if (a2) return;
      let i3 = await n3();
      if (a2) {
        i3.dispose();
        return;
      }
      y2 = i3, b2 = Fa(y2, t4, { dpr: [1, 1.6] });
      let s4 = await Ec(y2, b2, `fft-ocean-live`, o2);
      if (a2) {
        try {
          Ic(s4);
        } catch {
        }
        return;
      }
      x2 = s4, ee2 = b2.onResize(re2), m2 = i3.onError((e) => {
        a2 || w2(e);
      }), i3.gpu.lost.then((e) => {
        a2 || w2(Error(`WebGPU device lost: ${e.reason}`));
      });
      let c3 = As(y2);
      l2 = Ms(y2, (e) => {
        if (!(a2 || u3 || !x2 || !b2)) try {
          Mc(x2, c3.time * Q2.simulation.timeScale), oe2(x2.particles, c3.time), Pc(e, x2, b2), v2 || (v2 = true, h2());
        } catch (e2) {
          w2(e2);
        }
      }, r2 === void 0 ? void 0 : { fps: r2 });
    })().catch((e) => {
      a2 || w2(e);
    });
    function ae2(e) {
      u3 = e;
    }
    function oe2(e, t5) {
      s3[0] += (c2[0] - s3[0]) * bc, s3[1] += (c2[1] - s3[1]) * bc, s3[2] += (c2[2] - s3[2]) * xc, s3[2] < 1e-3 && c2[2] === 0 && (s3[2] = 0), e.set({ u: { cursor: [s3[0], s3[1], s3[2], t5] } });
    }
    function T2(e) {
      a2 || (c2 = [e[0], e[1], e[2]]);
    }
    function se2(e) {
      a2 || (o2 = e, x2 && jc(x2, e));
    }
    return { ready: ie2, firstFrame: _2, dispose: te2, setColors: se2, setPaused: ae2, setPointer: T2 };
  }
  async function Ec(e, t4, n2, r2 = lc) {
    let i2 = [];
    try {
      let a2 = Dc(e, t4, n2, r2, (e2) => (i2.push(e2), e2));
      return await kc(a2, t4), a2;
    } catch (e2) {
      try {
        Lc(i2);
      } catch {
      }
      throw e2;
    }
  }
  function Dc(e, t4, n2, r2, i2) {
    let a2 = (t5, r3, a3) => i2(Xs(e, { size: r3, format: a3, label: `${n2}-${t5}` })), o2 = (e2) => a2(e2, [512, 512], Sc), s3 = { noise: o2(`noise`), h0: o2(`h0`), spectrum: o2(`spectrum`), ping: o2(`ping`), pong: o2(`pong`), normalFoam: o2(`normal-foam`) }, c2 = Fc(t4.size), l2 = a2(`scene`, zc(t4.size), Cc), u3 = a2(`bright`, c2[0], Cc), d2 = a2(`composite`, c2[0], Cc), f2 = fo(e, { minFilter: `linear`, magFilter: `linear` }), p2 = $2(e, sc, `${n2}-noise`), m2 = $2(e, _c, `${n2}-initial-spectrum`, { u: { resolution: 512, size: Q2.simulation.oceanSize, windSpeed: Q2.simulation.windSpeed, windAngle: Q2.simulation.windAngle, amplitude: Q2.simulation.amplitude }, u_noise: s3.noise }), h2 = $2(e, yc, `${n2}-spectrum`, { u: { resolution: 512, size: Q2.simulation.oceanSize, time: 0, choppiness: Q2.simulation.choppiness }, u_initialSpectrum: s3.h0 }), g2 = { spectrum: s3.spectrum, ping: s3.ping, pong: s3.pong }, _2 = fc().map((t5) => ({ spec: t5, effect: $2(e, gc, `${n2}-ifft-${t5.index}-${t5.horizontal ? `h` : `v`}`, { u: { resolution: 512, subtransformSize: t5.subtransformSize, horizontal: +!!t5.horizontal }, u_input: g2[t5.input] }), output: g2[t5.output] })), v2 = _2[_2.length - 1].output, y2 = $2(e, vc, `${n2}-normal-foam`, { u: { resolution: 512, worldSize: Q2.simulation.worldSize, displacementScale: Q2.simulation.displacementScale, foamThreshold: Q2.simulation.foamThreshold }, u_displacement: v2 }), b2 = xo(e, { shader: pc, vertices: 6, instances: 512 * 512, blend: { color: { src: `src-alpha`, dst: `one` }, alpha: { src: `one`, dst: `one` } }, label: `${n2}-particles` }).set({ u_displacement: v2, u_normalFoam: s3.normalFoam });
    Nc(b2, t4);
    let x2 = $2(e, $s, `${n2}-bloom-bright`, { uniforms: { luminosityThreshold: Q2.bloom.threshold, smoothWidth: Q2.bloom.smoothWidth, _pad0: [0, 0] }, tDiffuse: l2, linearSampler: f2 }), ee2 = u3, S2 = c2.map((t5, r3) => {
      let i3 = a2(`bloom-h${r3}`, t5, Cc), o3 = a2(`bloom-v${r3}`, t5, Cc), s4 = Q2.bloom.kernelRadii[r3], c3 = Oc(e, `${n2}-blur-h${r3}`, ee2, i3, f2, [1, 0], s4), l3 = Oc(e, `${n2}-blur-v${r3}`, i3, o3, f2, [0, 1], s4);
      return ee2 = o3, { horizontal: i3, vertical: o3, horizontalEffect: c3, verticalEffect: l3 };
    }), C2 = $2(e, ec, `${n2}-bloom-composite`, { uniforms: { bloomStrength: Q2.bloom.strength, bloomRadius: Q2.bloom.radius, bloomFactors0: [1, 0.8, 0.6, 0.4], bloomFactors1: [0.2, 0, 0, 0], _pad0: [0, 0] }, blurTexture1: S2[0].vertical, blurTexture2: S2[1].vertical, blurTexture3: S2[2].vertical, blurTexture4: S2[3].vertical, blurTexture5: S2[4].vertical, linearSampler: f2 }), te2 = $2(e, mc, `${n2}-present`, { uniforms: Ac(r2), sceneHDR: l2, bloomTexture: d2, linearSampler: f2 });
    return { simulation: s3, scene: l2, bloom: { bright: u3, composite: d2, levels: S2 }, effects: { noise: p2, initialSpectrum: m2, evolveSpectrum: h2, normals: y2, bright: x2, composite: C2, present: te2 }, ifft: _2, particles: b2, needsInitialSpectrum: true };
  }
  function $2(e, t4, n2, r2) {
    let i2 = Cs(e, t4, { label: n2 });
    return r2 ? i2.set(r2) : i2;
  }
  function Oc(e, t4, n2, r2, i2, a2, o2) {
    let s3 = Cs(e, Qs, { label: t4 }), c2 = rc(o2);
    return s3.set({ uniforms: { direction: a2, invSize: r2.texelSize, gaussianCoefficients0: c2.slice(0, 4), gaussianCoefficients1: c2.slice(4, 8), gaussianCoefficients2: c2.slice(8, 12), gaussianCoefficients3: c2.slice(12, 16), gaussianCoefficients4: c2.slice(16, 20), gaussianCoefficients5: c2.slice(20, 24) }, colorTexture: n2, linearSampler: i2 }), s3;
  }
  async function kc(e, t4) {
    let n2 = (await Promise.allSettled([e.effects.noise.compile(e.simulation.noise), e.effects.initialSpectrum.compile(e.simulation.h0), e.effects.evolveSpectrum.compile(e.simulation.spectrum), ...e.ifft.map(({ effect: e2, output: t5 }) => e2.compile(t5)), e.effects.normals.compile(e.simulation.normalFoam), e.particles.compile(e.scene), e.effects.bright.compile(e.bloom.bright), ...e.bloom.levels.flatMap((e2) => [e2.horizontalEffect.compile(e2.horizontal), e2.verticalEffect.compile(e2.vertical)]), e.effects.composite.compile(e.bloom.composite), e.effects.present.compile({ colors: [t4.format] })])).find((e2) => e2.status === `rejected`);
    if (n2) throw n2.reason;
  }
  function Ac(e) {
    return { fgColor: [...e.fg, 1], bgColor: [...e.bg, 1], brightness: Q2.present.brightness, _pad: [0, 0, 0] };
  }
  function jc(e, t4) {
    e.effects.present.set({ uniforms: Ac(t4) });
  }
  function Mc(e, t4) {
    e.effects.evolveSpectrum.set({ u: { time: t4 * Q2.simulation.spectrumTimeScale } });
  }
  function Nc(e, t4) {
    let n2 = ic(t4.size), r2 = Q2;
    e.set({ u: { view: n2.view, projection: n2.projection, viewport: [t4.size[0], t4.size[1], 1, 512], world: [r2.simulation.worldSize, r2.simulation.displacementScale, r2.particles.pointSize, 0], fade: [r2.particles.fadeNear, r2.particles.fadeFar, r2.particles.fadePower, 0], oceanColor: r2.particles.oceanColor, neonColor: r2.particles.neonColor, foamColor: r2.particles.foamColor, cursor: [0, 0, 0, 0] } });
  }
  function Pc(e, t4, n2) {
    let r2 = (t5, n3) => e.pass({ target: t5, clear: wc }, (e2) => e2.draw(n3));
    t4.needsInitialSpectrum && (t4.needsInitialSpectrum = (r2(t4.simulation.noise, t4.effects.noise), r2(t4.simulation.h0, t4.effects.initialSpectrum), false)), r2(t4.simulation.spectrum, t4.effects.evolveSpectrum);
    for (let e2 of t4.ifft) r2(e2.output, e2.effect);
    r2(t4.simulation.normalFoam, t4.effects.normals), r2(t4.scene, t4.particles), r2(t4.bloom.bright, t4.effects.bright);
    for (let e2 of t4.bloom.levels) r2(e2.horizontal, e2.horizontalEffect), r2(e2.vertical, e2.verticalEffect);
    r2(t4.bloom.composite, t4.effects.composite), r2(n2, t4.effects.present);
  }
  function Fc(e) {
    let t4 = Math.max(1, Math.round(e[0] / 2)), n2 = Math.max(1, Math.round(e[1] / 2));
    return Array.from({ length: Q2.bloom.levels }, () => {
      let e2 = [t4, n2];
      return t4 = Math.max(1, Math.round(t4 / 2)), n2 = Math.max(1, Math.round(n2 / 2)), e2;
    });
  }
  function Ic(e) {
    Lc([...Object.values(e.simulation), e.scene, e.bloom.bright, e.bloom.composite, ...e.bloom.levels.flatMap((e2) => [e2.horizontal, e2.vertical])]);
  }
  function Lc(e) {
    Rc([...e].reverse().map((e2) => () => e2.color.destroy()));
  }
  function Rc(e) {
    let t4, n2 = false;
    for (let r2 of e) try {
      r2();
    } catch (e2) {
      n2 || (t4 = e2), n2 = true;
    }
    if (n2) throw t4;
  }
  function zc(e) {
    return [Math.max(1, Math.floor(e[0])), Math.max(1, Math.floor(e[1]))];
  }
  function Bc(e, t4) {
    return e[0] === t4[0] && e[1] === t4[1];
  }
  return __toCommonJS(renderer_BGT6fWD2_exports);
})();
