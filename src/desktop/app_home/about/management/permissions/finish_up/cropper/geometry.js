// Square-crop specialization of React Image Crop's containment, pointer-anchor,
// and keyboard helpers. Copyright (c) 2015 Dominic Tobias; ISC license in
// geometry.LICENSE. Reference behavior is pinned in the cropper source contract.
// No DOM, owner state, request dispatch, or wire rounding belongs in this module.
const workflowCropGeometry = (() => {
  const clamp = (value, low, high) => Math.min(Math.max(value, low), high);
  const west = ord => ord === 'nw' || ord === 'sw';
  const north = ord => ord === 'nw' || ord === 'ne';

  function initial(width, height) {
    // Responsive native adaptation: an image narrower than the reference's
    // minimum must still contain its selection. Normal reference sizes match.
    const size = Math.min(width, height, Math.max(128, Math.min(width, height) * .95));
    return {x: Math.max(0, (width - size) / 2), y: Math.max(0, (height - size) / 2), width: size, height: size};
  }

  function constrained(crop, ord, width, height, minimum) {
    const c = {...crop};
    // The reference aspect=1 path makes minWidth authoritative here. Pointer
    // resize first resolves the minimum against both dimensions; keys do not.
    const min = Math.min(minimum, width);
    const maxWidth = Math.min(2000, width), maxHeight = 2000;
    if (c.y < 0) { c.height = Math.max(c.height + c.y, min); c.y = 0; }
    if (c.x < 0) { c.width = Math.max(c.width + c.x, min); c.x = 0; }
    const overflowX = width - (c.x + c.width);
    if (overflowX < 0) { c.x = Math.min(c.x, width - min); c.width += overflowX; }
    const overflowY = height - (c.y + c.height);
    if (overflowY < 0) { c.y = Math.min(c.y, height - min); c.height += overflowY; }
    if (c.width < min) { if (west(ord)) c.x -= min - c.width; c.width = min; }
    if (c.height < min) { if (north(ord)) c.y -= min - c.height; c.height = min; }
    if (c.width > maxWidth) { if (west(ord)) c.x -= maxWidth - c.width; c.width = maxWidth; }
    if (c.height > maxHeight) { if (north(ord)) c.y -= maxHeight - c.height; c.height = maxHeight; }
    const aspect = c.width / c.height;
    if (aspect < 1) {
      const h = Math.max(c.width, min);
      if (north(ord)) c.y -= h - c.height;
      c.height = h;
    } else if (aspect > 1) {
      const w = Math.max(c.height, min);
      if (west(ord)) c.x -= w - c.width;
      c.width = w;
    }
    return c;
  }
  const contain = (crop, ord, width, height) => constrained(crop, ord, width, height, 128);

  function start(crop, ord, clientX, clientY, rect) {
    let anchorX = crop.x, anchorY = crop.y, startX = clientX, startY = clientY;
    if (ord) {
      const x = clientX - rect.x, y = clientY - rect.y;
      let offsetX, offsetY;
      if (ord === 'ne' || ord === 'e') {
        offsetX = x - (crop.x + crop.width); offsetY = y - crop.y;
        anchorY = crop.y + crop.height;
      } else if (ord === 'se' || ord === 's') {
        offsetX = x - (crop.x + crop.width); offsetY = y - (crop.y + crop.height);
      } else if (ord === 'sw' || ord === 'w') {
        offsetX = x - crop.x; offsetY = y - (crop.y + crop.height);
        anchorX = crop.x + crop.width;
      } else {
        offsetX = x - crop.x; offsetY = y - crop.y;
        anchorX = crop.x + crop.width; anchorY = crop.y + crop.height;
      }
      startX = anchorX + rect.x + offsetX; startY = anchorY + rect.y + offsetY;
    }
    return {crop: {...crop}, ord, anchorX, anchorY, startX, startY};
  }

  function move(g, clientX, clientY, rect) {
    let dx = clientX - g.startX, dy = clientY - g.startY;
    if (!g.ord) return {...g.crop,
      x: clamp(g.anchorX + dx, 0, rect.width - g.crop.width),
      y: clamp(g.anchorY + dy, 0, rect.height - g.crop.height)};
    const min = Math.min(128, rect.height);
    const left = ['nw', 'w', 'sw'].includes(g.ord);
    const above = ['nw', 'n', 'ne'].includes(g.ord);
    const region = left ? (above ? 'nw' : 'sw') : (above ? 'ne' : 'se');
    if (left) dx = Math.min(dx, -min);
    if (above) dy = Math.min(dy, -min);
    // For a fixed square, horizontal displacement controls the size even for
    // north/south handles. Preserve the reference's opposite anchor and offset.
    let c;
    if (region === 'ne') c = {x:g.anchorX, y:g.anchorY-dx, width:dx, height:dx};
    else if (region === 'se') c = {x:g.anchorX, y:g.anchorY, width:dx, height:dx};
    else if (region === 'sw') c = {x:g.anchorX+dx, y:g.anchorY, width:Math.abs(dx), height:Math.abs(dx)};
    else c = {x:g.anchorX+dx, y:g.anchorY-Math.abs(dx), width:Math.abs(dx), height:Math.abs(dx)};
    c = constrained(c, region, rect.width, rect.height, min);
    c.x = clamp(c.x, 0, rect.width-c.width); c.y = clamp(c.y, 0, rect.height-c.height);
    return c;
  }

  function key(crop, ord, key, step, width, height) {
    const c = {...crop};
    if (!ord) {
      if (key === 'ArrowLeft') c.x -= step;
      if (key === 'ArrowRight') c.x += step;
      if (key === 'ArrowUp') c.y -= step;
      if (key === 'ArrowDown') c.y += step;
      c.x = clamp(c.x, 0, width-c.width); c.y = clamp(c.y, 0, height-c.height);
      return c;
    }
    const horizontal = key === 'ArrowLeft' || key === 'ArrowRight';
    const vertical = key === 'ArrowUp' || key === 'ArrowDown';
    if (!horizontal && !vertical) return c;
    const delta = key === 'ArrowLeft' || key === 'ArrowUp' ? step : -step;
    if (ord === 'nw') { c.x-=delta; c.y-=delta; c.width+=delta; c.height+=delta; }
    else if (ord === 'se') { c.width-=delta; c.height-=delta; }
    else if (ord === 'ne') {
      const d = horizontal ? -delta : delta;
      c.y-=d; c.width+=d; c.height+=d;
    } else if (ord === 'sw') {
      const d = horizontal ? delta : -delta;
      c.x-=d; c.width+=d; c.height+=d;
    } else if (horizontal && ord === 'w') { c.x-=delta; c.width+=delta; }
    else if (horizontal && ord === 'e') c.width-=delta;
    else if (vertical && ord === 'n') { c.y-=delta; c.height+=delta; }
    else if (vertical && ord === 's') c.height-=delta;
    return contain(c, ord, width, height);
  }

  function full(crop, displayWidth, displayHeight, naturalWidth, naturalHeight) {
    const sx = Math.max(1, naturalWidth/displayWidth), sy = Math.max(1, naturalHeight/displayHeight);
    return {x:crop.x*sx, y:crop.y*sy, size:crop.width*sx};
  }

  // Native precision controls use full-image coordinates but obey the same
  // displayed square/minimum as pointer controls. Invalid values retain state.
  function numeric(value, field, next, displayWidth, displayHeight, naturalWidth, naturalHeight) {
    const sx = Math.max(1, naturalWidth/displayWidth), sy = Math.max(1, naturalHeight/displayHeight);
    const updated = {...value};
    if (['x','y','size'].includes(field) && Number.isFinite(next)) updated[field] = next;
    const min = Math.min(128, displayWidth, displayHeight);
    const size = clamp(updated.size/sx, min, Math.min(2000, displayWidth, displayHeight));
    return {x:clamp(updated.x/sx,0,displayWidth-size), y:clamp(updated.y/sy,0,displayHeight-size), width:size, height:size};
  }
  return Object.freeze({initial, contain, start, move, key, full, numeric});
})();
