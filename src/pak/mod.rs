pub(crate) mod model;
pub(crate) mod scene;

mod anim;
mod bitmap;
mod bitmap_font;
mod data_ref;
mod id;
mod pak_buf;

// TODO: Remove ErrorKind!
pub use {
    self::{
        anim::{Animation, Channel},
        bitmap::{Bitmap, Format as BitmapFormat},
        bitmap_font::BitmapFont,
        id::{AnimationId, BitmapFontId, BitmapId, BlobId, MaterialId, ModelId, SceneId, TextId},
        model::Model,
        pak_buf::PakBuf,
        scene::Scene,
    },
    bincode::ErrorKind,
};

use {
    self::id::Id,
    bincode::deserialize_from,
    brotli::{CompressorReader as BrotliReader, CompressorWriter as BrotliWriter},
    gfx_hal::IndexType as GfxHalIndexType,
    serde::{de::DeserializeOwned, Deserialize, Serialize},
    snap::{read::FrameDecoder as SnapReader, write::FrameEncoder as SnapWriter},
    std::{
        borrow::Cow,
        env::current_exe,
        fs::File,
        io::{BufReader, Error, Read, Seek, SeekFrom, Write},
        path::Path,
    },
};

#[cfg(debug_assertions)]
use {
    num_format::{Locale, ToFormattedString},
    std::time::Instant,
};

pub(self) use self::data_ref::DataRef;

fn read_exact<R: Read + Seek>(reader: &mut R, pos: u64, len: usize) -> Vec<u8> {
    // Unsafely create a buffer of uninitialized data (this is faster)
    let mut buf = Vec::with_capacity(len);
    unsafe {
        buf.set_len(len);
    }

    // Read the data into our buffer
    reader.seek(SeekFrom::Start(pos)).unwrap(); // TODO: Unwrapping IO reads!!
    reader.read_exact(&mut buf).unwrap();

    buf
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub(crate) struct BrotliCompression {
    pub buf_size: usize,
    pub quality: u32,
    pub window_size: u32,
}

impl Default for BrotliCompression {
    fn default() -> Self {
        Self {
            buf_size: 4096,
            quality: 10,
            window_size: 20,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub(crate) enum Compression {
    Brotli(BrotliCompression),
    Snap,
}

impl Compression {
    fn reader<'r, R: Read + 'r>(compression: Option<Self>, reader: R) -> Box<dyn Read + 'r> {
        match compression {
            Some(compression) => match compression {
                Compression::Brotli(b) => Box::new(BrotliReader::new(
                    reader,
                    b.buf_size,
                    b.quality,
                    b.window_size,
                )),
                Compression::Snap => Box::new(SnapReader::new(reader)),
            },
            _ => Box::new(reader),
        }
    }

    fn writer<'w, W: Write + 'w>(compression: Option<Self>, writer: W) -> Box<dyn Write + 'w> {
        match compression {
            Some(compression) => match compression {
                Compression::Brotli(b) => Box::new(BrotliWriter::new(
                    writer,
                    b.buf_size,
                    b.quality,
                    b.window_size,
                )),
                Compression::Snap => Box::new(SnapWriter::new(writer)),
            },
            _ => Box::new(writer),
        }
    }
}

impl Default for Compression {
    fn default() -> Self {
        Self::Brotli(Default::default())
    }
}

#[derive(Clone, Copy, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum IndexType {
    U16,
    U32,
}

impl From<IndexType> for GfxHalIndexType {
    fn from(val: IndexType) -> Self {
        match val {
            IndexType::U16 => Self::U16,
            IndexType::U32 => Self::U32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Material {
    pub color: BitmapId,
    pub metal_rough: BitmapId,
    pub normal: BitmapId,
}

pub struct Pak<R>
where
    R: Read + Seek,
{
    buf: PakBuf,
    compression: Option<Compression>,
    reader: R,
}

impl Pak<BufReader<File>> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let current_dir = current_exe()?.parent().unwrap().to_path_buf(); // TODO: Unwrap
        let pak_path = current_dir.join(&path);
        let pak_file = File::open(&pak_path)?;
        let mut reader = BufReader::new(pak_file);

        #[cfg(debug_assertions)]
        let started = Instant::now();

        let skip = {
            let mut buf: [u8; 4] = Default::default();
            reader.read_exact(&mut buf).unwrap();
            u32::from_ne_bytes(buf)
        };

        let compression: Option<Compression> = deserialize_from(&mut reader).unwrap();

        reader.seek(SeekFrom::Start(skip as _))?;

        let buf = {
            let mut reader = Compression::reader(compression, &mut reader);
            deserialize_from(&mut reader).unwrap()
        };

        #[cfg(debug_assertions)]
        {
            let elapsed = Instant::now() - started;
            if elapsed.as_millis() > 0 {
                info!(
                    "PakBuf::open took {}ms",
                    elapsed.as_millis().to_formatted_string(&Locale::en)
                );
            }
        }

        Ok(Self {
            buf,
            compression,
            reader,
        })
    }
}

impl<R> Pak<R>
where
    R: Read + Seek,
{
    pub fn animation_id<K: AsRef<str>>(&self, key: K) -> Option<AnimationId> {
        if let Some(Id::Animation(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn bitmap_id<K: AsRef<str>>(&self, key: K) -> Option<BitmapId> {
        if let Some(Id::Bitmap(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn bitmap_font_id<K: AsRef<str>>(&self, key: K) -> Option<BitmapFontId> {
        if let Some(Id::BitmapFont(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn blob_id<K: AsRef<str>>(&self, key: K) -> Option<BlobId> {
        if let Some(Id::Blob(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn material_id<K: AsRef<str>>(&self, key: K) -> Option<MaterialId> {
        if let Some(Id::Material(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn material<K: AsRef<str>>(&self, key: K) -> Material {
        let id = self.material_id(key).unwrap();
        self.material_with_id(id)
    }

    pub fn material_with_id(&self, id: MaterialId) -> Material {
        self.buf.material(id)
    }

    pub fn model_id<K: AsRef<str>>(&self, key: K) -> Option<ModelId> {
        if let Some(Id::Model(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn scene_id<K: AsRef<str>>(&self, key: K) -> Option<SceneId> {
        if let Some(Id::Scene(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn text<K: AsRef<str>>(&self, key: K) -> Cow<str> {
        // TODO: Pick proper user locale or best guess; use additional libs to detect!
        self.buf.text_locale(key, "en-US")
    }

    pub fn text_id<K: AsRef<str>>(&self, key: K) -> Option<TextId> {
        if let Some(Id::Text(id)) = self.buf.id(key) {
            Some(id)
        } else {
            None
        }
    }

    pub fn text_locale<K: AsRef<str>, L: AsRef<str>>(&self, key: K, locale: L) -> Cow<str> {
        self.buf.text_locale(key, locale)
    }

    pub fn text_raw<K: AsRef<str>>(&self, key: K) -> Cow<str> {
        self.buf.text(key)
    }

    fn read<T: DeserializeOwned>(&mut self, pos: u64, len: usize) -> T {
        let buf = read_exact(&mut self.reader, pos, len);
        let reader = Compression::reader(self.compression, buf.as_slice());

        deserialize_from(reader).unwrap()
    }

    pub fn read_animation(&mut self, id: AnimationId) -> Animation {
        let (pos, len) = self.buf.animation(id);
        self.read(pos, len)
    }

    pub fn read_bitmap(&mut self, id: BitmapId) -> Bitmap {
        let (pos, len) = self.buf.bitmap(id);
        self.read(pos, len)
    }

    pub fn read_bitmap_font(&mut self, id: BitmapFontId) -> BitmapFont {
        let (pos, len) = self.buf.bitmap_font(id);
        self.read(pos, len)
    }

    pub fn read_blob(&mut self, id: BlobId) -> Vec<u8> {
        let (pos, len) = self.buf.blob(id);
        self.read(pos, len)
    }

    pub fn read_model(&mut self, id: ModelId) -> Model {
        let (pos, len) = self.buf.model(id);
        self.read(pos, len)
    }

    pub fn read_scene(&mut self, id: SceneId) -> Scene {
        let (pos, len) = self.buf.scene(id);
        self.read(pos, len)
    }
}
