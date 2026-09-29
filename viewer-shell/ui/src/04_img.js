/* =====================================================================================
   The two things about images the page still does for itself.

   It used to hold decoders for DDS (DXT1/3/5 and every uncompressed bit layout) and TGA
   — a second reader of two formats the engine now reads, and both of them ran on the
   thread the window is drawn on. Measured, a 1024² DXT1 cost 11.7 ms there, and a busy
   cell touches hundreds of textures. `img.rs` owns that knowledge now, the same move as
   §17b's one NIF reader, and what is left here is the part the browser genuinely does
   better than we would: PNG, JPEG and BMP, which `createImageBitmap` decodes off the
   main thread anyway.
   ===================================================================================== */

async function decodeViaBrowser(buf, mime){
  const blob=new Blob([buf],{type:mime});
  const bmp=await createImageBitmap(blob);
  const cv=document.createElement('canvas'); cv.width=bmp.width; cv.height=bmp.height;
  const cx=cv.getContext('2d'); cx.drawImage(bmp,0,0);
  const id=cx.getImageData(0,0,cv.width,cv.height);
  bmp.close&&bmp.close();
  return {w:cv.width,h:cv.height,data:new Uint8Array(id.data.buffer)};
}

function rgbaToDataURL(img,maxSide){
  const s=Math.min(1,(maxSide||64)/Math.max(img.w,img.h));
  const cw=Math.max(1,Math.round(img.w*s)), ch=Math.max(1,Math.round(img.h*s));
  const src=document.createElement('canvas'); src.width=img.w; src.height=img.h;
  const sc=src.getContext('2d');
  /* From the view's own range, not from its whole buffer. The pixels now arrive as a
     `Uint8Array` pointing into the middle of the engine's reply — `data.buffer` is that
     entire payload, and handing it to `ImageData` would draw the header. */
  const px = img.data instanceof Uint8ClampedArray
    ? img.data
    : new Uint8ClampedArray(img.data.buffer, img.data.byteOffset||0, img.w*img.h*4);
  sc.putImageData(new ImageData(px,img.w,img.h),0,0);
  if(s===1) return src.toDataURL();
  const dst=document.createElement('canvas'); dst.width=cw; dst.height=ch;
  const dc=dst.getContext('2d'); dc.imageSmoothingEnabled=false; dc.drawImage(src,0,0,cw,ch);
  return dst.toDataURL();
}
