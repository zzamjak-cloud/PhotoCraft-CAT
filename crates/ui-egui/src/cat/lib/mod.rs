//! CAT PhotoCraft 조작 라이브러리 (CAT `lib/` · JP_Plugin `src/lib/` 대응).
//! 엔진 명령 문자열은 이 모듈에만 둔다. 기능 파일(`features/`)은 여기 함수만 쓴다.

pub mod core;
pub mod doc;
pub mod export;
pub mod guide;
pub mod layer;
pub mod pixels;
pub mod select;
pub mod shape;

pub use self::core::CatResult;
