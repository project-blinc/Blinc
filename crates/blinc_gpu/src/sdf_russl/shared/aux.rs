//! The auxiliary data: variable-length per-primitive data such as 3D group shape descriptors,
//! polygon clip vertices, gradient stop tables and ABI clip headers. Tiers 0 and 1 read a storage
//! buffer; tier 2 reads an `RGBA32F` texture 1024 texels wide and `ceil(len / 1024)` rows tall.
//! `AUX_SLOT` is the bind group slot: each pipeline's layout puts it somewhere else.

russl::russl! {
    #[pipeline] const TIER: u32 = 0;
    #[pipeline] const AUX_SLOT: u32 = 5;

    #if TIER == 2
        #if AUX_SLOT == 3
            #[group(0)] #[binding(3)] static AUX_TEX: Texture2D<f32>;
        #elif AUX_SLOT == 4
            #[group(0)] #[binding(4)] static AUX_TEX: Texture2D<f32>;
        #elif AUX_SLOT == 7
            #[group(0)] #[binding(7)] static AUX_TEX: Texture2D<f32>;
        #else
            #[group(0)] #[binding(5)] static AUX_TEX: Texture2D<f32>;
        #endif

        // Auxiliary data vec4 `index`.
        pub fn aux(index: u32) -> Vec4 {
            let i = index as i32;
            return AUX_TEX.load(IVec2::new(i % 1024, i / 1024), 0);
        }
    #else
        #if AUX_SLOT == 3
            #[group(0)] #[binding(3)] static AUX_DATA: Storage<[Vec4]>;
        #elif AUX_SLOT == 4
            #[group(0)] #[binding(4)] static AUX_DATA: Storage<[Vec4]>;
        #elif AUX_SLOT == 7
            #[group(0)] #[binding(7)] static AUX_DATA: Storage<[Vec4]>;
        #else
            #[group(0)] #[binding(5)] static AUX_DATA: Storage<[Vec4]>;
        #endif

        // Auxiliary data vec4 `index`.
        pub fn aux(index: u32) -> Vec4 {
            return AUX_DATA[index];
        }
    #endif
}
