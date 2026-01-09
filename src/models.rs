#[derive(Clone)]
pub struct Model {
	pub id: &'static str,
	pub name: &'static str,
	pub size: &'static str,
	pub url: &'static str,
}

pub const MODELS: &[Model] = &[
	Model {
		id: "ggml-tiny.en.bin",
		name: "Tiny (EN)",
		size: "78 MB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
	},
	Model {
		id: "ggml-tiny.bin",
		name: "Tiny",
		size: "78 MB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
	},
	Model {
		id: "ggml-base.en.bin",
		name: "Base (EN)",
		size: "148 MB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
	},
	Model {
		id: "ggml-base.bin",
		name: "Base",
		size: "148 MB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
	},
	Model {
		id: "ggml-small.en.bin",
		name: "Small (EN)",
		size: "488 MB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
	},
	Model {
		id: "ggml-small.bin",
		name: "Small",
		size: "488 MB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
	},
	Model {
		id: "ggml-medium.en.bin",
		name: "Medium (EN)",
		size: "1.53 GB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
	},
	Model {
		id: "ggml-medium.bin",
		name: "Medium",
		size: "1.53 GB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
	},
	Model {
		id: "ggml-large-v1.bin",
		name: "Large v1",
		size: "3.09 GB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v1.bin",
	},
	Model {
		id: "ggml-large-v2.bin",
		name: "Large v2",
		size: "3.09 GB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v2.bin",
	},
	Model {
		id: "ggml-large-v3.bin",
		name: "Large v3",
		size: "3.1 GB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
	},
	Model {
		id: "ggml-large-v3-turbo.bin",
		name: "Large v3 Turbo",
		size: "1.62 GB",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
	},
];
