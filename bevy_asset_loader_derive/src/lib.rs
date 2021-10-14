//! This crate adds support for auto deriving ``bevy_asset_loader::AssetCollection``
//!
//! You do not have to use it directly. Just import ``AssetCollection`` from ``bevy_asset_loader``
//! and use ``#[derive(AssetCollection)]`` to derive the trait.

extern crate proc_macro;

mod assets;

use proc_macro::TokenStream;
use std::option::Option::Some;
use std::result::Result::{Err, Ok};

use crate::assets::*;
use proc_macro2::Ident;
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{Data, Field, Fields, Lit, Meta, NestedMeta};

/// Derive macro for AssetCollection
///
/// The helper attribute ``asset`` can be used to define the path to the asset file.
#[proc_macro_derive(AssetCollection, attributes(asset))]
pub fn asset_collection_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_asset_collection(ast)
        .unwrap_or_else(to_compile_errors)
        .into()
}

const ASSET_ATTRIBUTE: &str = "asset";
const PATH_ATTRIBUTE: &str = "path";

const TEXTURE_ATLAS_ATTRIBUTE: &str = "texture_atlas";
struct TextureAtlasAttribute;
impl TextureAtlasAttribute {
    pub const TILE_SIZE_X: &'static str = "tile_size_x";
    pub const TILE_SIZE_Y: &'static str = "tile_size_y";
    pub const COLUMNS: &'static str = "columns";
    pub const ROWS: &'static str = "rows";
    pub const PADDING_X: &'static str = "padding_x";
    pub const PADDING_Y: &'static str = "padding_y";
}

const COLOR_MATERIAL_ATTRIBUTE: &str = "color_material";

#[derive(Default)]
struct AssetBuilder {
    field_ident: Option<Ident>,
    asset_path: Option<String>,
    tile_size_x: Option<f32>,
    tile_size_y: Option<f32>,
    columns: Option<usize>,
    rows: Option<usize>,
    padding_x: f32,
    padding_y: f32,
    is_color_material: bool,
}

impl AssetBuilder {
    fn build(self) -> Result<Asset, Vec<ParseFieldError>> {
        let mut missing_fields = vec![];
        if self.tile_size_x.is_none() {
            missing_fields.push(format!(
                "{}/{}",
                TEXTURE_ATLAS_ATTRIBUTE,
                TextureAtlasAttribute::TILE_SIZE_X
            ));
        }
        if self.tile_size_y.is_none() {
            missing_fields.push(format!(
                "{}/{}",
                TEXTURE_ATLAS_ATTRIBUTE,
                TextureAtlasAttribute::TILE_SIZE_Y
            ));
        }
        if self.columns.is_none() {
            missing_fields.push(format!(
                "{}/{}",
                TEXTURE_ATLAS_ATTRIBUTE,
                TextureAtlasAttribute::COLUMNS
            ));
        }
        if self.rows.is_none() {
            missing_fields.push(format!(
                "{}/{}",
                TEXTURE_ATLAS_ATTRIBUTE,
                TextureAtlasAttribute::ROWS
            ));
        }
        if self.field_ident.is_none() || self.asset_path.is_none() {
            return Err(vec![ParseFieldError::NoAttributes]);
        }
        if missing_fields.len() == 4 {
            let asset = BasicAsset {
                field_ident: self.field_ident.unwrap(),
                asset_path: self.asset_path.unwrap(),
            };
            if self.is_color_material {
                return Ok(Asset::ColorMaterial(asset));
            }
            return Ok(Asset::Basic(asset));
        }
        if missing_fields.is_empty() {
            return Ok(Asset::TextureAtlas(TextureAtlasAsset {
                field_ident: self.field_ident.unwrap(),
                asset_path: self.asset_path.unwrap(),
                tile_size_x: self.tile_size_x.unwrap(),
                tile_size_y: self.tile_size_y.unwrap(),
                columns: self.columns.unwrap(),
                rows: self.rows.unwrap(),
                padding_x: self.padding_x,
                padding_y: self.padding_y,
            }));
        }
        Err(vec![ParseFieldError::MissingAttributes(missing_fields)])
    }
}

fn impl_asset_collection(
    ast: syn::DeriveInput,
) -> Result<proc_macro2::TokenStream, Vec<syn::Error>> {
    let name = &ast.ident;

    let mut default_fields: Vec<Ident> = vec![];
    let mut assets: Vec<Asset> = vec![];
    if let Data::Struct(ref data_struct) = ast.data {
        if let Fields::Named(ref named_fields) = data_struct.fields {
            let mut compile_errors = vec![];
            for field in named_fields.named.iter() {
                let asset = parse_field(field);
                match asset {
                    Ok(asset) => assets.push(asset),
                    Err(errors) => {
                        for error in errors {
                            match error {
                                ParseFieldError::NoAttributes => {
                                    default_fields.push(field.clone().ident.unwrap())
                                }
                                ParseFieldError::MissingAttributes(missing_attributes) => {
                                    compile_errors.push(syn::Error::new_spanned(
                                        field.into_token_stream(),
                                        format!(
                                            "Field is missing asset attributes: {}",
                                            missing_attributes.join(", ")
                                        ),
                                    ));
                                }
                                ParseFieldError::WrongAttributeType(token_stream, expected) => {
                                    compile_errors.push(syn::Error::new_spanned(
                                        token_stream,
                                        format!("Wrong attribute type. Expected '{}'", expected),
                                    ));
                                }
                                ParseFieldError::UnknownAttributeType(token_stream) => {
                                    compile_errors.push(syn::Error::new_spanned(
                                        token_stream,
                                        "Unknown attribute type",
                                    ));
                                }
                                ParseFieldError::UnknownAttribute(token_stream) => {
                                    compile_errors.push(syn::Error::new_spanned(
                                        token_stream,
                                        "Unknown attribute",
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            if !compile_errors.is_empty() {
                return Err(compile_errors);
            }
        } else {
            return Err(vec![syn::Error::new_spanned(
                data_struct.fields.clone().into_token_stream(),
                "only named fields are supported to derive AssetCollection",
            )]);
        }
    } else {
        return Err(vec![syn::Error::new_spanned(
            &ast.into_token_stream(),
            "AssetCollection can only be derived for a struct",
        )]);
    }

    let mut asset_creation = assets.iter().fold(quote!(), |es, asset| match asset {
        Asset::Basic(basic) => {
            let field_ident = basic.field_ident.clone();
            let asset_path = basic.asset_path.clone();
            quote!(#es#field_ident : asset_server.get_handle(#asset_path),)
        }
        Asset::ColorMaterial(basic) => {
            let field_ident = basic.field_ident.clone();
            let asset_path = basic.asset_path.clone();
            quote!(#es#field_ident : materials.add(asset_server.get_handle(#asset_path).into()),)
        }
        Asset::TextureAtlas(texture_asset) => {
            let field_ident = texture_asset.field_ident.clone();
            let asset_path = texture_asset.asset_path.clone();
            let tile_size_x = texture_asset.tile_size_x;
            let tile_size_y = texture_asset.tile_size_y;
            let columns = texture_asset.columns;
            let rows = texture_asset.rows;
            let padding_x = texture_asset.padding_x;
            let padding_y = texture_asset.padding_y;
            quote!(
                #es#field_ident : {
                atlases.add(TextureAtlas::from_grid_with_padding(
                    asset_server.get_handle(#asset_path),
                    Vec2::new(#tile_size_x, #tile_size_y),
                    #columns,
                    #rows,
                    Vec2::new(#padding_x, #padding_y),
                ))},
            )
        }
    });
    asset_creation.append_all(default_fields.iter().fold(
        quote!(),
        |es, ident| quote! (#es#ident : Default::default()),
    ));

    let asset_loading = assets.iter().fold(quote!(), |es, asset| match asset {
        Asset::Basic(asset) => {
            let asset_path = asset.asset_path.clone();
            quote!(#es handles.push(asset_server.load_untyped(#asset_path));)
        }
        Asset::ColorMaterial(asset) => {
            let asset_path = asset.asset_path.clone();
            quote!(#es handles.push(asset_server.load_untyped(#asset_path));)
        }
        Asset::TextureAtlas(asset) => {
            let asset_path = asset.asset_path.clone();
            quote!(#es handles.push(asset_server.load_untyped(#asset_path));)
        }
    });

    #[allow(unused_mut)]
    let mut conditional_asset_collections = quote! {};

    #[cfg(feature = "sprite")]
    {
        conditional_asset_collections = quote! {
        #conditional_asset_collections
                let mut materials = cell
                    .get_resource_mut::<Assets<ColorMaterial>>()
                    .expect("Cannot get Assets<ColorMaterial>");
        };
    }

    #[cfg(feature = "render")]
    {
        conditional_asset_collections = quote! {
        #conditional_asset_collections
                let mut atlases = cell
                    .get_resource_mut::<Assets<TextureAtlas>>()
                    .expect("Cannot get Assets<TextureAtlas>");
        };
    }

    let create_function = quote! {
            fn create(world: &mut World) -> Self {
                let cell = world.cell();
                let asset_server = cell.get_resource::<AssetServer>().expect("Cannot get AssetServer");
                #conditional_asset_collections
                #name {
                    #asset_creation
                }
            }
    };

    let impl_asset_collection = quote! {
        #[automatically_derived]
        #[allow(unused_variables)]
        impl AssetCollection for #name {
            #create_function

            fn load(world: &mut World) -> Vec<HandleUntyped> {
                let asset_server = world.get_resource::<AssetServer>().expect("Cannot get AssetServer");
                let mut handles = vec![];
                #asset_loading
                handles
            }
        }
    };
    Ok(impl_asset_collection)
}

enum ParseFieldError {
    NoAttributes,
    WrongAttributeType(proc_macro2::TokenStream, &'static str),
    UnknownAttributeType(proc_macro2::TokenStream),
    UnknownAttribute(proc_macro2::TokenStream),
    MissingAttributes(Vec<String>),
}

fn parse_field(field: &Field) -> Result<Asset, Vec<ParseFieldError>> {
    let mut builder = AssetBuilder::default();
    let mut errors = vec![];
    for attr in field.attrs.iter() {
        if let syn::Meta::List(ref asset_meta_list) = attr.parse_meta().unwrap() {
            if *asset_meta_list.path.get_ident().unwrap() != ASSET_ATTRIBUTE {
                continue;
            }

            for attribute in asset_meta_list.nested.iter() {
                if let NestedMeta::Meta(Meta::NameValue(ref named_value)) = attribute {
                    let path = named_value.path.get_ident().unwrap().clone();

                    if path == PATH_ATTRIBUTE {
                        if let Lit::Str(path_literal) = &named_value.lit {
                            builder.asset_path = Some(path_literal.value());
                            builder.field_ident = Some(field.clone().ident.unwrap());
                        } else {
                            errors.push(ParseFieldError::WrongAttributeType(
                                named_value.clone().into_token_stream(),
                                "str",
                            ));
                        }
                    } else {
                        errors.push(ParseFieldError::UnknownAttribute(
                            named_value.clone().into_token_stream(),
                        ))
                    }
                } else if let NestedMeta::Meta(Meta::Path(ref meta_path)) = attribute {
                    let path = meta_path.get_ident().unwrap().clone();
                    if path == COLOR_MATERIAL_ATTRIBUTE {
                        builder.is_color_material = true;
                    } else {
                        errors.push(ParseFieldError::UnknownAttribute(
                            meta_path.clone().into_token_stream(),
                        ))
                    }
                } else if let NestedMeta::Meta(Meta::List(ref meta_list)) = attribute {
                    let path = meta_list.path.get_ident().unwrap().clone();
                    if path == TEXTURE_ATLAS_ATTRIBUTE {
                        for attribute in meta_list.nested.iter() {
                            if let NestedMeta::Meta(Meta::NameValue(ref named_value)) = attribute {
                                let path = named_value.path.get_ident().unwrap().clone();
                                if path == TextureAtlasAttribute::TILE_SIZE_X {
                                    if let Lit::Float(width) = &named_value.lit {
                                        builder.tile_size_x =
                                            Some(width.base10_parse::<f32>().unwrap());
                                    } else {
                                        errors.push(ParseFieldError::WrongAttributeType(
                                            named_value.clone().into_token_stream(),
                                            "float",
                                        ));
                                    }
                                } else if path == TextureAtlasAttribute::TILE_SIZE_Y {
                                    if let Lit::Float(height) = &named_value.lit {
                                        builder.tile_size_y =
                                            Some(height.base10_parse::<f32>().unwrap());
                                    } else {
                                        errors.push(ParseFieldError::WrongAttributeType(
                                            named_value.clone().into_token_stream(),
                                            "float",
                                        ));
                                    }
                                } else if path == TextureAtlasAttribute::COLUMNS {
                                    if let Lit::Int(columns) = &named_value.lit {
                                        builder.columns =
                                            Some(columns.base10_parse::<usize>().unwrap());
                                    } else {
                                        errors.push(ParseFieldError::WrongAttributeType(
                                            named_value.clone().into_token_stream(),
                                            "integer",
                                        ));
                                    }
                                } else if path == TextureAtlasAttribute::ROWS {
                                    if let Lit::Int(rows) = &named_value.lit {
                                        builder.rows = Some(rows.base10_parse::<usize>().unwrap());
                                    } else {
                                        errors.push(ParseFieldError::WrongAttributeType(
                                            named_value.clone().into_token_stream(),
                                            "integer",
                                        ));
                                    }
                                } else if path == TextureAtlasAttribute::PADDING_X {
                                    if let Lit::Float(padding_x) = &named_value.lit {
                                        builder.padding_x =
                                            padding_x.base10_parse::<f32>().unwrap();
                                    } else {
                                        errors.push(ParseFieldError::WrongAttributeType(
                                            named_value.clone().into_token_stream(),
                                            "float",
                                        ));
                                    }
                                } else if path == TextureAtlasAttribute::PADDING_Y {
                                    if let Lit::Float(padding_y) = &named_value.lit {
                                        builder.padding_y =
                                            padding_y.base10_parse::<f32>().unwrap();
                                    } else {
                                        errors.push(ParseFieldError::WrongAttributeType(
                                            named_value.clone().into_token_stream(),
                                            "float",
                                        ));
                                    }
                                } else {
                                    errors.push(ParseFieldError::UnknownAttribute(
                                        named_value.clone().into_token_stream(),
                                    ));
                                }
                            }
                        }
                    } else {
                        errors.push(ParseFieldError::UnknownAttribute(
                            meta_list.clone().into_token_stream(),
                        ))
                    }
                } else {
                    errors.push(ParseFieldError::UnknownAttributeType(
                        attribute.clone().into_token_stream(),
                    ));
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    builder.build()
}

fn to_compile_errors(errors: Vec<syn::Error>) -> proc_macro2::TokenStream {
    let compile_errors = errors.iter().map(syn::Error::to_compile_error);
    quote!(#(#compile_errors)*)
}
