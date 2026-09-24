// The whole SDK as one `window.ChimeSDK` global. It ships as CommonJS, so
// esbuild cannot tree-shake it: an audio-only entry comes out the same size.
export * from 'amazon-chime-sdk-js';
