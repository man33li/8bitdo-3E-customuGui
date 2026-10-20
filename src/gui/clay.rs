//! Single-threaded Clay context. Text is copied into a fixed frame arena; no
//! borrowed string or image pointer survives rendering. No Rust frame allocation.
use std::{fmt::{self, Write}, marker::PhantomData, rc::Rc, sync::atomic::{AtomicBool, Ordering}};
use super::ffi as f;

static ACTIVE: AtomicBool = AtomicBool::new(false);
pub type Color = f::Clay_Color;
pub type Bounds = f::Clay_BoundingBox;
pub const fn rgb(r: u8, g: u8, b: u8) -> Color { Color { r: r as f32, g: g as f32, b: b as f32, a: 255.0 } }
fn string(text: &str) -> f::Clay_String { f::Clay_String { isStaticallyAllocated: false, length: text.len() as i32, chars: text.as_ptr().cast() } }
pub fn id(name: &'static str, index: u32) -> f::Clay_ElementId { unsafe { f::Clay_GetElementIdWithIndex(string(name), index) } }
#[derive(Clone, Copy)]
pub enum Size { Fit, Grow, Fixed(f32), Percent(f32) }
fn axis(size: Size) -> f::Clay_SizingAxis {
    let mut a = f::Clay_SizingAxis::default();
    a.type_ = match size { Size::Fit => 0, Size::Grow => 1, Size::Percent(v) => { a.size.percent = v; 2 }, Size::Fixed(v) => { a.size.minMax = f::Clay_SizingMinMax { min: v, max: v }; 3 } };
    a
}
#[derive(Clone, Copy)]
pub struct Style(pub(super) f::Clay_ElementDeclaration);
impl Style {
    pub fn column(width: Size, height: Size) -> Self { let mut s = Self::row(width, height); s.0.layout.layoutDirection = 1; s }
    pub fn row(width: Size, height: Size) -> Self { let mut d = f::Clay_ElementDeclaration::default(); d.layout.sizing = f::Clay_Sizing { width: axis(width), height: axis(height) }; Self(d) }
    pub fn padding(mut self, n: u16) -> Self { self.0.layout.padding = f::Clay_Padding { left:n, right:n, top:n, bottom:n }; self }
    pub fn gap(mut self, n: u16) -> Self { self.0.layout.childGap=n; self }
    pub fn background(mut self, color: Color) -> Self { self.0.backgroundColor=color; self }
    pub fn rounded(mut self, n: f32) -> Self { self.0.cornerRadius=f::Clay_CornerRadius { topLeft:n, topRight:n, bottomLeft:n, bottomRight:n }; self }
    pub fn border(mut self, color: Color, width: u16) -> Self { self.0.border=f::Clay_BorderElementConfig { color, width:f::Clay_BorderWidth { left:width,right:width,top:width,bottom:width,betweenChildren:0 } }; self }
    pub fn centered(mut self) -> Self { self.0.layout.childAlignment=f::Clay_ChildAlignment { x:2,y:2 }; self }
    pub fn floating(mut self, parent: Option<f::Clay_ElementId>, x:f32, y:f32, z:i16) -> Self { self.0.floating.attachTo=if parent.is_some(){2}else{3}; self.0.floating.parentId=parent.map_or(0,|p|p.id); self.0.floating.offset=f::Clay_Vector2{x,y}; self.0.floating.zIndex=z; self }
}
pub struct Clay { _arena: Vec<u64>, scratch: Box<[u8; 65536]>, used: usize, images:[f::Texture2D;64],image_count:usize, _thread: PhantomData<Rc<()>> }
impl Clay {
    pub fn new(width:f32,height:f32) -> crate::Result<Self> {
        if ACTIVE.swap(true,Ordering::AcqRel) { return Err(crate::invalid("only one GUI context may be active")); }
        let bytes=unsafe{f::Clay_MinMemorySize()} as usize;
        let mut arena=vec![0u64;bytes.div_ceil(8)];
        unsafe { let a=f::Clay_CreateArenaWithCapacityAndMemory(arena.len()*8,arena.as_mut_ptr().cast()); f::Clay_Initialize(a,f::Clay_Dimensions{width,height},f::Clay_ErrorHandler { errorHandlerFunction:Some(error),userData:std::ptr::null_mut() }); }
        Ok(Self { _arena:arena,scratch:Box::new([0;65536]),used:0,images:[f::Texture2D::default();64],image_count:0,_thread:PhantomData })
    }
    pub fn begin(&mut self,width:f32,height:f32,pointer:[f32;2],down:bool,scroll:[f32;2],dt:f32) -> Frame<'_> {
        self.used=0;
        self.image_count=0;
        unsafe { f::Clay_SetLayoutDimensions(f::Clay_Dimensions{width,height}); f::Clay_SetPointerState(f::Clay_Vector2{x:pointer[0],y:pointer[1]},down); f::Clay_UpdateScrollContainers(false,f::Clay_Vector2{x:scroll[0],y:scroll[1]},dt); f::Clay_BeginLayout(); }
        Frame { clay:self,depth:0,finished:false }
    }
}
impl Drop for Clay { fn drop(&mut self) { unsafe{f::Clay_SetCurrentContext(std::ptr::null_mut());} ACTIVE.store(false,Ordering::Release); } }
unsafe extern "C" fn error(data:f::Clay_ErrorData) { let bytes=unsafe{std::slice::from_raw_parts(data.errorText.chars.cast::<u8>(),data.errorText.length.max(0) as usize)}; eprintln!("Clay: {}",String::from_utf8_lossy(bytes)); }
pub struct Frame<'a> { clay:&'a mut Clay,depth:usize,finished:bool }
impl Frame<'_> {
    pub fn element(&mut self,name:&'static str,index:u32,style:Style,children:impl FnOnce(&mut Self)) {
        unsafe { f::Clay__OpenElementWithId(id(name,index)); f::Clay__ConfigureOpenElement(style.0); }
        self.depth+=1; children(self); self.depth-=1; unsafe { f::Clay__CloseElement(); }
    }
    pub fn scroll(&mut self,name:&'static str,style:Style,children:impl FnOnce(&mut Self)) {
        unsafe { f::Clay__OpenElementWithId(id(name,0)); }
        let mut style=style; style.0.clip.vertical=true; style.0.clip.childOffset=unsafe{f::Clay_GetScrollOffset()};
        unsafe { f::Clay__ConfigureOpenElement(style.0); }
        self.depth+=1;children(self);
        let data=unsafe{f::Clay_GetScrollContainerData(id(name,0))};
        if let Some(view)=self.bounds(name,0){
            if data.found&&!data.scrollPosition.is_null()&&data.contentDimensions.height>view.height+1.0{
                let height=(view.height*view.height/data.contentDimensions.height).max(24.0);
                let progress=(-unsafe{(*data.scrollPosition).y}/(data.contentDimensions.height-view.height)).clamp(0.0,1.0);
                self.element("scrollbar",id(name,0).id,Style::column(Size::Fixed(4.0),Size::Fixed(height)).background(rgb(117,137,164)).rounded(2.0).floating(Some(id(name,0)),view.width-5.0,progress*(view.height-height),20),|_|{});
            }
        }
        self.depth-=1;unsafe{f::Clay__CloseElement();}
    }
    pub fn text(&mut self,text:impl fmt::Display,size:u16,color:Color) {
        struct Buffer<'a>{bytes:&'a mut [u8],len:usize}
        impl Write for Buffer<'_> { fn write_str(&mut self,s:&str)->fmt::Result { if self.len+s.len()>self.bytes.len(){return Err(fmt::Error)} self.bytes[self.len..self.len+s.len()].copy_from_slice(s.as_bytes()); self.len+=s.len(); Ok(()) } }
        let start=self.clay.used;
        let mut buffer=Buffer{bytes:&mut self.clay.scratch[start..],len:0};
        write!(&mut buffer,"{text}").expect("GUI text exceeded 64 KiB frame arena");
        let length=buffer.len; self.clay.used+=length;
        let s=f::Clay_String{isStaticallyAllocated:false,length:length as i32,chars:self.clay.scratch[start..].as_ptr().cast()};
        let config=f::Clay_TextElementConfig{fontSize:size,textColor:color,..Default::default()};
        unsafe{f::Clay__OpenTextElement(s,config)};
    }
    pub fn image(&mut self,name:&'static str,style:Style,texture:&Texture) { self.image_index(name,0,style,texture); }
    pub fn image_index(&mut self,name:&'static str,index:u32,style:Style,texture:&Texture) { assert!(self.clay.image_count<self.clay.images.len(),"too many GUI images");let slot=self.clay.image_count;self.clay.image_count+=1;self.clay.images[slot]=texture.raw;let mut style=style; style.0.image.imageData=(&mut self.clay.images[slot] as *mut f::Texture2D).cast(); self.element(name,index,style,|_|{}); }
    pub fn hovered(&self,name:&'static str,index:u32)->bool{unsafe{f::Clay_PointerOver(id(name,index))}}
    pub fn bounds(&self,name:&'static str,index:u32)->Option<Bounds>{let d=unsafe{f::Clay_GetElementData(id(name,index))}; d.found.then_some(d.boundingBox)}
    pub fn reveal_in_scroll(&mut self,container:&'static str,name:&'static str,index:u32) {
        let Some(view)=self.bounds(container,0)else{return};let Some(item)=self.bounds(name,index)else{return};
        if item.x<view.x||item.x>=view.x+view.width{return}
        let data=unsafe{f::Clay_GetScrollContainerData(id(container,0))};
        if data.found&&!data.scrollPosition.is_null(){unsafe{let position=&mut *data.scrollPosition;if item.y<view.y{position.y+=view.y-item.y;}else if item.y+item.height>view.y+view.height{position.y-=item.y+item.height-view.y-view.height;}position.y=position.y.clamp((view.height-data.contentDimensions.height).min(0.0),0.0);}}
    }
    pub(super) fn render(mut self,dt:f32,font:&mut f::Font,scale:f32)->f64 { let start=std::time::Instant::now();let commands=unsafe{f::Clay_EndLayout(dt)};let layout_ms=start.elapsed().as_secs_f64()*1000.0;unsafe{f::Ultimate_Render(commands,font,scale)}; self.finished=true;layout_ms }
}
impl Drop for Frame<'_>{fn drop(&mut self){if !self.finished { unsafe{for _ in 0..self.depth{f::Clay__CloseElement();} f::Clay_EndLayout(0.0);} }}}
pub struct Texture { raw:f::Texture2D }
impl Texture {
    #[cfg(test)]
    pub(super) fn layout_only()->Self{Self{raw:f::Texture2D::default()}}
    pub(super) fn png(bytes:&[u8])->crate::Result<Self>{unsafe{let image=f::LoadImageFromMemory(c".png".as_ptr(),bytes.as_ptr(),bytes.len() as i32); if image.data.is_null(){return Err(crate::invalid("artwork failed to decode"))} let raw=f::LoadTextureFromImage(image); f::UnloadImage(image); if raw.id==0{return Err(crate::invalid("artwork failed to upload"))} f::SetTextureFilter(raw,f::TEXTURE_FILTER_BILINEAR as i32); Ok(Self{raw})}}
    pub(super) fn fit(&self,width:f32,height:f32)->[f32;2]{
        if self.raw.width<=0||self.raw.height<=0{return [width,height];}
        let scale=(width/self.raw.width as f32).min(height/self.raw.height as f32);
        [self.raw.width as f32*scale,self.raw.height as f32*scale]
    }
}
impl Drop for Texture{fn drop(&mut self){if self.raw.id!=0{unsafe{f::UnloadTexture(self.raw)}}}}
