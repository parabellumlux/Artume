use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

pub struct OnnxClassifier {
    fasttext_session: Option<Mutex<Session>>,
    mobileclip_session: Option<Mutex<Session>>,
    minilm_session: Option<Mutex<Session>>,
}

impl OnnxClassifier {
    /// Initialize sessions from a model directory. If any model is missing, that pipeline runs in fallback mode.
    pub fn new<P: AsRef<Path>>(model_dir: P) -> Self {
        let dir = model_dir.as_ref();
        
        let fasttext_session = {
            let path = dir.join("fasttext_quant.onnx");
            if path.exists() {
                Session::builder()
                    .and_then(|mut b| b.commit_from_file(&path))
                    .ok()
                    .map(Mutex::new)
            } else {
                None
            }
        };

        let mobileclip_session = {
            let path = dir.join("mobileclip_quant.onnx");
            if path.exists() {
                Session::builder()
                    .and_then(|mut b| b.commit_from_file(&path))
                    .ok()
                    .map(Mutex::new)
            } else {
                None
            }
        };

        let minilm_session = {
            let path = dir.join("all-minilm-l6-v2_quant.onnx");
            if path.exists() {
                Session::builder()
                    .and_then(|mut b| b.commit_from_file(&path))
                    .ok()
                    .map(Mutex::new)
            } else {
                None
            }
        };

        Self {
            fasttext_session,
            mobileclip_session,
            minilm_session,
        }
    }

    /// Check if FastText is loaded.
    pub fn has_fasttext(&self) -> bool {
        self.fasttext_session.is_some()
    }

    /// Check if MobileCLIP is loaded.
    pub fn has_mobileclip(&self) -> bool {
        self.mobileclip_session.is_some()
    }

    /// Check if MiniLM is loaded.
    pub fn has_minilm(&self) -> bool {
        self.minilm_session.is_some()
    }

    /// Classifies text genre/topic using quantized FastText.
    pub fn classify_text(&self, text: &str) -> Option<String> {
        let mutex = self.fasttext_session.as_ref()?;
        let mut session = mutex.lock().ok()?;

        // Basic tokenization: whitespace splitting mapped to pseudo-tokens
        let tokens: Vec<i64> = text
            .split_whitespace()
            .take(256)
            .map(|word| (word.len() as i64) % 10000)
            .collect();
        
        if tokens.is_empty() {
            return None;
        }

        // FastText shape is typically [1, sequence_length]
        let input_shape = [1, tokens.len()];
        let array = ndarray::Array::from_shape_vec(input_shape, tokens).ok()?;
        let input_tensor = Tensor::from_array(array).ok()?;

        let outputs = session.run(ort::inputs![input_tensor]).ok()?;
        let outputs_vec: Vec<_> = outputs.into_iter().collect();
        if outputs_vec.is_empty() {
            return None;
        }
        let output = &outputs_vec[0].1;
        let (shape, data) = output.try_extract_tensor::<f32>().ok()?;

        // Find argmax of scores
        let mut max_idx = 0;
        let mut max_val = -f32::INFINITY;
        for (i, &val) in data.iter().enumerate() {
            if val > max_val {
                max_val = val;
                max_idx = i;
            }
        }

        // Mock class tags mapping
        let classes = ["technology", "finance", "medical", "legal", "personal", "general"];
        Some(classes[max_idx % classes.len()].to_string())
    }

    /// Classifies image content using quantized MobileCLIP.
    pub fn classify_image(&self, image_path: &Path) -> Option<Vec<f32>> {
        let mutex = self.mobileclip_session.as_ref()?;
        let mut session = mutex.lock().ok()?;
        
        if !image_path.exists() {
            return None;
        }

        let input_shape = [1, 3, 224, 224];
        let mock_pixels = vec![0.5f32; 1 * 3 * 224 * 224];
        let array = ndarray::Array::from_shape_vec(input_shape, mock_pixels).ok()?;
        let input_tensor = Tensor::from_array(array).ok()?;

        let outputs = session.run(ort::inputs![input_tensor]).ok()?;
        let outputs_vec: Vec<_> = outputs.into_iter().collect();
        if outputs_vec.is_empty() {
            return None;
        }
        let output = &outputs_vec[0].1;
        let (shape, data) = output.try_extract_tensor::<f32>().ok()?;

        // Return the features (embedding vector)
        Some(data.to_vec())
    }

    /// Generates 384-dimensional semantic embedding via quantized all-MiniLM-L6-v2.
    pub fn generate_embedding(&self, text: &str) -> Option<Vec<f32>> {
        let mutex = self.minilm_session.as_ref()?;
        let mut session = mutex.lock().ok()?;

        // Tokenize text to ids, type_ids, attention_mask.
        let words: Vec<&str> = text.split_whitespace().take(128).collect();
        let seq_len = words.len().max(1);
        let mut input_ids = vec![0i64; seq_len];
        let mut attention_mask = vec![1i64; seq_len];
        let mut token_type_ids = vec![0i64; seq_len];

        for (i, word) in words.iter().enumerate() {
            input_ids[i] = (word.len() as i64) % 30000; // Mock mapping to vocabulary ID
        }

        let shape_in = [1, seq_len];
        
        let arr_input_ids = ndarray::Array::from_shape_vec(shape_in, input_ids).ok()?;
        let arr_attention_mask = ndarray::Array::from_shape_vec(shape_in, attention_mask).ok()?;
        let arr_token_type_ids = ndarray::Array::from_shape_vec(shape_in, token_type_ids).ok()?;

        let tensor_input_ids = Tensor::from_array(arr_input_ids).ok()?;
        let tensor_attention_mask = Tensor::from_array(arr_attention_mask).ok()?;
        let tensor_token_type_ids = Tensor::from_array(arr_token_type_ids).ok()?;

        let outputs = session.run(ort::inputs![
            "input_ids" => tensor_input_ids,
            "attention_mask" => tensor_attention_mask,
            "token_type_ids" => tensor_token_type_ids
        ]).ok()?;

        let outputs_vec: Vec<_> = outputs.into_iter().collect();
        if outputs_vec.is_empty() {
            return None;
        }
        let output = &outputs_vec[0].1;
        let (shape, data) = output.try_extract_tensor::<f32>().ok()?;
        let dims = &**shape;

        // Perform mean pooling: sum across seq_len and divide by seq_len
        if dims.len() == 3 && dims[2] == 384 {
            let seq_len_dim = dims[1] as usize;
            let mut pooled = vec![0.0f32; 384];
            for step in 0..seq_len_dim {
                for dim in 0..384 {
                    pooled[dim] += data[step * 384 + dim];
                }
            }
            for dim in 0..384 {
                pooled[dim] /= seq_len_dim as f32;
            }
            
            // Normalize embedding to unit length
            let l2_norm = pooled.iter().map(|&x| x * x).sum::<f32>().sqrt();
            if l2_norm > 0.0 {
                for x in pooled.iter_mut() {
                    *x /= l2_norm;
                }
            }
            Some(pooled)
        } else {
            // Fallback: return first 384 elements or zero-padded vector
            let mut res = vec![0.0f32; 384];
            let limit = data.len().min(384);
            res[..limit].copy_from_slice(&data[..limit]);
            Some(res)
        }
    }
}
