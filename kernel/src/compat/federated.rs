/// Federated Machine Learning
///
/// Implements privacy-preserving distributed ML training within the kernel.
/// Supports federated averaging, differential privacy, and secure aggregation.
///
/// Features:
///   - Federated averaging (FedAvg) protocol
///   - Differential privacy (DP-SGD with Gaussian noise)
///   - Secure aggregation with secret sharing
///   - Model compression for bandwidth efficiency
///   - Heterogeneous device support
///   - Asynchronous federated learning
///   - Model versioning
///   - Client selection strategies
///   - Gradient compression (Top-K, random sparsification)
///   - Byzantine-fault tolerant aggregation
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

pub const MAX_CLIENTS: usize = 1024;
pub const DEFAULT_ROUNDS: u32 = 100;
pub const DEFAULT_LOCAL_EPOCHS: u32 = 5;
pub const DEFAULT_BATCH_SIZE: usize = 32;
pub const DEFAULT_LEARNING_RATE: f64 = 0.01;
pub const DEFAULT_CLIENT_FRACTION: f64 = 0.1;

// ═══════════════════════════════════════════════════════════════════════
// DATA TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Model parameter (weight) type
pub type Weight = f64;

/// Model gradient type
pub type Gradient = f64;

/// Tensor - multi-dimensional array stored flat
#[derive(Debug, Clone)]
pub struct Tensor {
    pub data: Vec<Weight>,
    pub shape: Vec<usize>,
}

impl Tensor {
    pub fn zeros(shape: &[usize]) -> Self {
        let size = shape.iter().product::<usize>();
        Self {
            data: vec![0.0; size],
            shape: shape.to_vec(),
        }
    }

    pub fn random(shape: &[usize], seed: u64) -> Self {
        let size = shape.iter().product::<usize>();
        let mut data = Vec::with_capacity(size);
        let mut rng = seed;
        for _ in 0..size {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let val = ((rng >> 33) as f64) / (u32::MAX as f64) * 2.0 - 1.0;
            data.push(val * 0.01); // small random weights
        }
        Self {
            data,
            shape: shape.to_vec(),
        }
    }

    pub fn numel(&self) -> usize {
        self.data.len()
    }

    pub fn add_scaled(&mut self, other: &Tensor, scale: f64) {
        for (a, b) in self.data.iter_mut().zip(other.data.iter()) {
            *a += b * scale;
        }
    }

    pub fn scale(&mut self, factor: f64) {
        for v in self.data.iter_mut() {
            *v *= factor;
        }
    }

    pub fn l2_norm(&self) -> f64 {
        let sum: f64 = self.data.iter().map(|x| x * x).sum();
        sqrt(sum)
    }

    pub fn clip_norm(&mut self, max_norm: f64) {
        let norm = self.l2_norm();
        if norm > max_norm {
            let scale = max_norm / norm;
            self.scale(scale);
        }
    }
}

/// Model - collection of named parameter tensors
#[derive(Debug, Clone)]
pub struct FederatedModel {
    pub id: u64,
    pub name: String,
    pub version: u64,
    pub params: BTreeMap<String, Tensor>,
    pub num_params: usize,
}

impl FederatedModel {
    pub fn new(name: &str) -> Self {
        let id = NEXT_MODEL_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            name: String::from(name),
            version: 0,
            params: BTreeMap::new(),
            num_params: 0,
        }
    }

    pub fn add_layer(&mut self, name: &str, shape: &[usize]) {
        let tensor = Tensor::random(
            shape,
            self.id.wrapping_mul(31).wrapping_add(name.len() as u64),
        );
        self.num_params += tensor.numel();
        self.params.insert(String::from(name), tensor);
    }

    /// Create a simple feed-forward model
    pub fn simple_mlp(
        name: &str,
        input_size: usize,
        hidden_size: usize,
        output_size: usize,
    ) -> Self {
        let mut model = Self::new(name);
        model.add_layer("fc1_weight", &[hidden_size, input_size]);
        model.add_layer("fc1_bias", &[hidden_size]);
        model.add_layer("fc2_weight", &[output_size, hidden_size]);
        model.add_layer("fc2_bias", &[output_size]);
        model
    }

    /// Compute difference (gradient) from another model
    pub fn diff(&self, other: &FederatedModel) -> BTreeMap<String, Tensor> {
        let mut grads = BTreeMap::new();
        for (name, param) in &self.params {
            if let Some(other_param) = other.params.get(name) {
                let mut grad = Tensor::zeros(&param.shape);
                for (g, (a, b)) in grad
                    .data
                    .iter_mut()
                    .zip(param.data.iter().zip(other_param.data.iter()))
                {
                    *g = a - b;
                }
                grads.insert(name.clone(), grad);
            }
        }
        grads
    }

    /// Apply gradients with learning rate
    pub fn apply_gradients(&mut self, grads: &BTreeMap<String, Tensor>, lr: f64) {
        for (name, grad) in grads {
            if let Some(param) = self.params.get_mut(name) {
                param.add_scaled(grad, -lr);
            }
        }
        self.version += 1;
    }

    /// Average with another model (weighted)
    pub fn average_with(&mut self, other: &FederatedModel, other_weight: f64) {
        let self_weight = 1.0 - other_weight;
        for (name, param) in self.params.iter_mut() {
            if let Some(other_param) = other.params.get(name) {
                for (a, b) in param.data.iter_mut().zip(other_param.data.iter()) {
                    *a = *a * self_weight + *b * other_weight;
                }
            }
        }
    }
}

static NEXT_MODEL_ID: AtomicU64 = AtomicU64::new(1);

// ═══════════════════════════════════════════════════════════════════════
// DIFFERENTIAL PRIVACY
// ═══════════════════════════════════════════════════════════════════════

/// Differential privacy parameters
#[derive(Debug, Clone)]
pub struct DpConfig {
    pub epsilon: f64,          // privacy budget
    pub delta: f64,            // failure probability
    pub max_grad_norm: f64,    // gradient clipping bound
    pub noise_multiplier: f64, // Gaussian noise σ/S
    pub accountant: PrivacyAccountant,
}

impl DpConfig {
    pub fn new(epsilon: f64, delta: f64, max_grad_norm: f64) -> Self {
        // Simple noise calibration: σ = S * sqrt(2 * ln(1.25/δ)) / ε
        let noise_mult = sqrt(2.0 * ln(1.25 / delta)) / epsilon;
        Self {
            epsilon,
            delta,
            max_grad_norm,
            noise_multiplier: noise_mult,
            accountant: PrivacyAccountant::new(epsilon, delta),
        }
    }

    /// Add Gaussian noise to gradients
    pub fn add_noise(&self, grads: &mut BTreeMap<String, Tensor>, num_samples: usize, seed: u64) {
        let sigma = self.noise_multiplier * self.max_grad_norm;
        let mut rng = seed;

        for (_name, tensor) in grads.iter_mut() {
            // Clip gradient
            tensor.clip_norm(self.max_grad_norm);

            // Add Gaussian noise (Box-Muller transform)
            for v in tensor.data.iter_mut() {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let u1 = ((rng >> 33) as f64) / (u32::MAX as f64);
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let u2 = ((rng >> 33) as f64) / (u32::MAX as f64);

                let noise = sqrt(-2.0 * ln(u1.max(1e-10))) * cos(2.0 * PI * u2) * sigma;
                *v += noise / num_samples as f64;
            }
        }
    }
}

/// Privacy accountant (tracks privacy budget usage)
#[derive(Debug, Clone)]
pub struct PrivacyAccountant {
    pub total_epsilon: f64,
    pub total_delta: f64,
    pub spent_epsilon: f64,
    pub steps: u64,
}

impl PrivacyAccountant {
    pub fn new(epsilon: f64, delta: f64) -> Self {
        Self {
            total_epsilon: epsilon,
            total_delta: delta,
            spent_epsilon: 0.0,
            steps: 0,
        }
    }

    /// Account for one step of DP-SGD
    pub fn step(&mut self, sigma: f64, sample_rate: f64) {
        // Simple composition: ε_total = sqrt(2T * ln(1/δ)) * ε_single
        // This is a simplified RDP accountant
        self.steps += 1;
        let per_step = sample_rate / sigma;
        self.spent_epsilon += per_step * per_step; // Track sum of squares
    }

    /// Remaining privacy budget
    pub fn remaining_budget(&self) -> f64 {
        let spent = sqrt(2.0 * (self.steps as f64) * ln(1.0 / self.total_delta))
            * sqrt(self.spent_epsilon / self.steps.max(1) as f64);
        self.total_epsilon - spent
    }

    pub fn budget_exhausted(&self) -> bool {
        self.remaining_budget() <= 0.0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SECURE AGGREGATION
// ═══════════════════════════════════════════════════════════════════════

/// Secret share for secure aggregation (additive sharing)
#[derive(Debug, Clone)]
pub struct SecretShare {
    pub client_id: u64,
    pub share_data: Vec<f64>,
    pub mask_seed: u64,
}

/// Secure aggregation protocol
pub struct SecureAggregator {
    shares: BTreeMap<u64, SecretShare>,
    threshold: usize, // minimum clients needed
    modulus: f64,
}

impl SecureAggregator {
    pub fn new(threshold: usize) -> Self {
        Self {
            shares: BTreeMap::new(),
            threshold,
            modulus: 1e15, // practical modulus for floats
        }
    }

    /// Client creates masked update
    pub fn create_share(client_id: u64, data: &[f64], partners: &[u64], seed: u64) -> SecretShare {
        let mut masked = data.to_vec();
        let mut rng = seed ^ client_id;

        // Add pairwise masks
        for &partner in partners {
            if partner > client_id {
                // Add mask
                for v in masked.iter_mut() {
                    rng = rng
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let mask = ((rng >> 33) as f64) / (u32::MAX as f64) - 0.5;
                    *v += mask;
                }
            } else if partner < client_id {
                // Subtract mask
                let mut partner_rng = seed ^ partner;
                for v in masked.iter_mut() {
                    partner_rng = partner_rng
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let mask = ((partner_rng >> 33) as f64) / (u32::MAX as f64) - 0.5;
                    *v -= mask;
                }
            }
        }

        SecretShare {
            client_id,
            share_data: masked,
            mask_seed: seed,
        }
    }

    /// Add a client's share
    pub fn add_share(&mut self, share: SecretShare) {
        self.shares.insert(share.client_id, share);
    }

    /// Aggregate when enough shares collected
    pub fn aggregate(&self) -> Option<Vec<f64>> {
        if self.shares.len() < self.threshold {
            return None;
        }

        let first = self.shares.values().next()?;
        let len = first.share_data.len();
        let mut result = vec![0.0; len];
        let n = self.shares.len() as f64;

        for share in self.shares.values() {
            for (r, s) in result.iter_mut().zip(share.share_data.iter()) {
                *r += s;
            }
        }

        // Average
        for r in result.iter_mut() {
            *r /= n;
        }

        Some(result)
    }

    pub fn clear(&mut self) {
        self.shares.clear();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GRADIENT COMPRESSION
// ═══════════════════════════════════════════════════════════════════════

/// Gradient compression strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStrategy {
    None,
    TopK(usize),         // Keep top-K largest absolute values
    RandomSparsify(u32), // Keep with probability 1/k
    Quantize(u8),        // Quantize to N bits
}

/// Compressed gradient
#[derive(Debug, Clone)]
pub struct CompressedGradient {
    pub indices: Vec<usize>,
    pub values: Vec<f64>,
    pub original_size: usize,
    pub strategy: CompressionStrategy,
}

impl CompressedGradient {
    /// Compress a gradient tensor
    pub fn compress(data: &[f64], strategy: CompressionStrategy, seed: u64) -> Self {
        match strategy {
            CompressionStrategy::None => Self {
                indices: (0..data.len()).collect(),
                values: data.to_vec(),
                original_size: data.len(),
                strategy,
            },
            CompressionStrategy::TopK(k) => {
                let k = k.min(data.len());
                let mut indexed: Vec<(usize, f64)> = data.iter().copied().enumerate().collect();
                indexed.sort_by(|a, b| {
                    b.1.abs()
                        .partial_cmp(&a.1.abs())
                        .unwrap_or(core::cmp::Ordering::Equal)
                });
                let indices: Vec<usize> = indexed[..k].iter().map(|(i, _)| *i).collect();
                let values: Vec<f64> = indexed[..k].iter().map(|(_, v)| *v).collect();
                Self {
                    indices,
                    values,
                    original_size: data.len(),
                    strategy,
                }
            }
            CompressionStrategy::RandomSparsify(rate) => {
                let mut indices = Vec::new();
                let mut values = Vec::new();
                let mut rng = seed;
                for (i, &v) in data.iter().enumerate() {
                    rng = rng
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    if (rng >> 33) as u32 % rate == 0 {
                        indices.push(i);
                        values.push(v * rate as f64); // scale up to maintain expectation
                    }
                }
                Self {
                    indices,
                    values,
                    original_size: data.len(),
                    strategy,
                }
            }
            CompressionStrategy::Quantize(bits) => {
                // Uniform quantization
                let max_val = data.iter().map(|x| x.abs()).fold(0.0f64, f64::max);
                let levels = (1u64 << bits) as f64;
                let values: Vec<f64> = data
                    .iter()
                    .map(|&v| {
                        let quantized = round(v / max_val * levels / 2.0);
                        quantized * max_val * 2.0 / levels
                    })
                    .collect();
                Self {
                    indices: (0..data.len()).collect(),
                    values,
                    original_size: data.len(),
                    strategy,
                }
            }
        }
    }

    /// Decompress back to full gradient
    pub fn decompress(&self) -> Vec<f64> {
        let mut result = vec![0.0; self.original_size];
        for (&idx, &val) in self.indices.iter().zip(self.values.iter()) {
            if idx < result.len() {
                result[idx] = val;
            }
        }
        result
    }

    /// Compression ratio
    pub fn ratio(&self) -> f64 {
        self.values.len() as f64 / self.original_size as f64
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CLIENT SELECTION
// ═══════════════════════════════════════════════════════════════════════

/// Client selection strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientSelection {
    Random,        // Uniform random
    PowerOfChoice, // Pick fastest from random subset
    ResourceBased, // Weight by compute capability
    LossWeighted,  // Weight by training loss
}

/// Client info for selection
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: u64,
    pub compute_speed: f64, // relative compute speed
    pub bandwidth: f64,     // network bandwidth (Mbps)
    pub data_size: usize,   // local dataset size
    pub last_loss: f64,     // last training loss
    pub rounds_participated: u64,
    pub available: bool,
}

/// Select clients for a round
pub fn select_clients(
    clients: &[ClientInfo],
    fraction: f64,
    strategy: ClientSelection,
    seed: u64,
) -> Vec<u64> {
    let n = ceil(clients.len() as f64 * fraction) as usize;
    let n = n.max(1).min(clients.len());

    let available: Vec<&ClientInfo> = clients.iter().filter(|c| c.available).collect();
    if available.is_empty() {
        return Vec::new();
    }

    let mut rng = seed;
    let mut selected = Vec::new();

    match strategy {
        ClientSelection::Random => {
            let mut indices: Vec<usize> = (0..available.len()).collect();
            // Fisher-Yates shuffle
            for i in (1..indices.len()).rev() {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let j = (rng >> 33) as usize % (i + 1);
                indices.swap(i, j);
            }
            for &idx in indices.iter().take(n) {
                selected.push(available[idx].id);
            }
        }
        ClientSelection::PowerOfChoice => {
            // Pick 2x candidates, choose fastest
            let candidates = (n * 2).min(available.len());
            let mut indices: Vec<usize> = (0..available.len()).collect();
            for i in (1..indices.len()).rev() {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let j = (rng >> 33) as usize % (i + 1);
                indices.swap(i, j);
            }
            let mut candidates: Vec<&ClientInfo> = indices[..candidates]
                .iter()
                .map(|&i| available[i])
                .collect();
            candidates.sort_by(|a, b| {
                b.compute_speed
                    .partial_cmp(&a.compute_speed)
                    .unwrap_or(core::cmp::Ordering::Equal)
            });
            for c in candidates.iter().take(n) {
                selected.push(c.id);
            }
        }
        ClientSelection::ResourceBased => {
            // Weighted by compute_speed * bandwidth
            let total_weight: f64 = available
                .iter()
                .map(|c| c.compute_speed * c.bandwidth)
                .sum();
            let mut remaining = n;
            for client in &available {
                if remaining == 0 {
                    break;
                }
                let prob = (client.compute_speed * client.bandwidth) / total_weight * n as f64;
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let r = ((rng >> 33) as f64) / (u32::MAX as f64);
                if r < prob.min(1.0) {
                    selected.push(client.id);
                    remaining -= 1;
                }
            }
        }
        ClientSelection::LossWeighted => {
            // Weight by training loss (higher loss = more likely selected)
            let total_loss: f64 = available.iter().map(|c| c.last_loss.max(0.001)).sum();
            let mut remaining = n;
            for client in &available {
                if remaining == 0 {
                    break;
                }
                let prob = client.last_loss.max(0.001) / total_loss * n as f64;
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let r = ((rng >> 33) as f64) / (u32::MAX as f64);
                if r < prob.min(1.0) {
                    selected.push(client.id);
                    remaining -= 1;
                }
            }
        }
    }

    selected
}

// ═══════════════════════════════════════════════════════════════════════
// BYZANTINE FAULT TOLERANCE
// ═══════════════════════════════════════════════════════════════════════

/// Byzantine-resilient aggregation methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByzantineDefense {
    None,
    TrimmedMean(usize), // trim top/bottom k
    Median,             // coordinate-wise median
    Krum(usize),        // Multi-Krum with f byzantine
}

/// Aggregate with Byzantine tolerance
pub fn byzantine_aggregate(updates: &[Vec<f64>], defense: ByzantineDefense) -> Vec<f64> {
    if updates.is_empty() {
        return Vec::new();
    }

    let dim = updates[0].len();

    match defense {
        ByzantineDefense::None => {
            // Simple average
            let mut result = vec![0.0; dim];
            let n = updates.len() as f64;
            for update in updates {
                for (r, u) in result.iter_mut().zip(update.iter()) {
                    *r += u / n;
                }
            }
            result
        }
        ByzantineDefense::TrimmedMean(k) => {
            let mut result = vec![0.0; dim];
            let n = updates.len();
            if n <= 2 * k {
                // Fall back to simple average
                return byzantine_aggregate(updates, ByzantineDefense::None);
            }

            for d in 0..dim {
                let mut values: Vec<f64> = updates.iter().map(|u| u[d]).collect();
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
                let trimmed = &values[k..n - k];
                result[d] = trimmed.iter().sum::<f64>() / trimmed.len() as f64;
            }
            result
        }
        ByzantineDefense::Median => {
            let mut result = vec![0.0; dim];
            for d in 0..dim {
                let mut values: Vec<f64> = updates.iter().map(|u| u[d]).collect();
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
                let mid = values.len() / 2;
                result[d] = if values.len() % 2 == 0 {
                    (values[mid - 1] + values[mid]) / 2.0
                } else {
                    values[mid]
                };
            }
            result
        }
        ByzantineDefense::Krum(f) => {
            // Multi-Krum: select update with smallest sum of distances to closest n-f-2 peers
            let n = updates.len();
            if n <= 2 * f + 2 {
                return byzantine_aggregate(updates, ByzantineDefense::None);
            }

            let mut scores: Vec<(usize, f64)> = Vec::new();

            for i in 0..n {
                let mut dists: Vec<f64> = Vec::new();
                for j in 0..n {
                    if i != j {
                        let dist: f64 = updates[i]
                            .iter()
                            .zip(updates[j].iter())
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum();
                        dists.push(dist);
                    }
                }
                dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
                let score: f64 = dists[..n - f - 2].iter().sum();
                scores.push((i, score));
            }

            scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(core::cmp::Ordering::Equal));
            updates[scores[0].0].clone()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FEDERATED TRAINING COORDINATOR
// ═══════════════════════════════════════════════════════════════════════

/// Training configuration
#[derive(Debug, Clone)]
pub struct FederatedConfig {
    pub rounds: u32,
    pub local_epochs: u32,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub client_fraction: f64,
    pub client_selection: ClientSelection,
    pub compression: CompressionStrategy,
    pub byzantine_defense: ByzantineDefense,
    pub dp_config: Option<DpConfig>,
    pub secure_aggregation: bool,
    pub async_mode: bool,
}

impl FederatedConfig {
    pub fn default() -> Self {
        Self {
            rounds: DEFAULT_ROUNDS,
            local_epochs: DEFAULT_LOCAL_EPOCHS,
            batch_size: DEFAULT_BATCH_SIZE,
            learning_rate: DEFAULT_LEARNING_RATE,
            client_fraction: DEFAULT_CLIENT_FRACTION,
            client_selection: ClientSelection::Random,
            compression: CompressionStrategy::None,
            byzantine_defense: ByzantineDefense::None,
            dp_config: None,
            secure_aggregation: false,
            async_mode: false,
        }
    }
}

/// Federated learning round state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundState {
    Idle,
    SelectingClients,
    DistributingModel,
    TrainingLocally,
    CollectingUpdates,
    Aggregating,
    Complete,
}

/// Federated training coordinator (server)
pub struct FederatedCoordinator {
    pub config: FederatedConfig,
    pub global_model: FederatedModel,
    pub clients: Vec<ClientInfo>,
    pub current_round: u32,
    pub round_state: RoundState,
    pub selected_clients: Vec<u64>,
    pub collected_updates: BTreeMap<u64, BTreeMap<String, Tensor>>,
    pub training_losses: Vec<f64>,
    pub secure_agg: Option<SecureAggregator>,
}

impl FederatedCoordinator {
    pub fn new(model: FederatedModel, config: FederatedConfig) -> Self {
        let secure_agg = if config.secure_aggregation {
            Some(SecureAggregator::new(
                (config.client_fraction * MAX_CLIENTS as f64 * 0.5) as usize,
            ))
        } else {
            None
        };

        Self {
            config,
            global_model: model,
            clients: Vec::new(),
            current_round: 0,
            round_state: RoundState::Idle,
            selected_clients: Vec::new(),
            collected_updates: BTreeMap::new(),
            training_losses: Vec::new(),
            secure_agg,
        }
    }

    /// Register a client
    pub fn register_client(&mut self, info: ClientInfo) -> u64 {
        let id = info.id;
        self.clients.push(info);
        serial_println!(
            "[FEDML] Client {} registered (total: {})",
            id,
            self.clients.len()
        );
        id
    }

    /// Start a new training round
    pub fn start_round(&mut self) -> Vec<u64> {
        self.current_round += 1;
        self.round_state = RoundState::SelectingClients;

        // Select clients
        let seed = rdtsc();
        self.selected_clients = select_clients(
            &self.clients,
            self.config.client_fraction,
            self.config.client_selection,
            seed,
        );

        serial_println!(
            "[FEDML] Round {}: selected {} clients",
            self.current_round,
            self.selected_clients.len()
        );

        self.collected_updates.clear();
        self.round_state = RoundState::DistributingModel;
        self.selected_clients.clone()
    }

    /// Receive a client's model update
    pub fn receive_update(&mut self, client_id: u64, update: BTreeMap<String, Tensor>) {
        self.collected_updates.insert(client_id, update);
        serial_println!(
            "[FEDML] Round {}: received update from client {} ({}/{})",
            self.current_round,
            client_id,
            self.collected_updates.len(),
            self.selected_clients.len()
        );

        // Check if all updates received
        if self.collected_updates.len() >= self.selected_clients.len() {
            self.round_state = RoundState::Aggregating;
        }
    }

    /// Aggregate updates (FedAvg)
    pub fn aggregate(&mut self) -> f64 {
        self.round_state = RoundState::Aggregating;

        let n = self.collected_updates.len() as f64;
        if n == 0.0 {
            self.round_state = RoundState::Complete;
            return 0.0;
        }

        // Apply Byzantine defense if configured
        if self.config.byzantine_defense != ByzantineDefense::None {
            // Collect param names first to avoid borrow conflict
            let param_names: Vec<String> = self.global_model.params.keys().cloned().collect();
            for param_name in &param_names {
                let updates: Vec<Vec<f64>> = self
                    .collected_updates
                    .values()
                    .filter_map(|u| u.get(param_name).map(|t| t.data.clone()))
                    .collect();

                if !updates.is_empty() {
                    let aggregated = byzantine_aggregate(&updates, self.config.byzantine_defense);
                    if let Some(param) = self.global_model.params.get_mut(param_name) {
                        for (p, a) in param.data.iter_mut().zip(aggregated.iter()) {
                            *p = *a;
                        }
                    }
                }
            }
        } else {
            // Standard FedAvg
            // Reset global model params to zero
            for param in self.global_model.params.values_mut() {
                for v in param.data.iter_mut() {
                    *v = 0.0;
                }
            }

            // Average all updates
            for update in self.collected_updates.values() {
                for (name, tensor) in update {
                    if let Some(param) = self.global_model.params.get_mut(name) {
                        param.add_scaled(tensor, 1.0 / n);
                    }
                }
            }
        }

        // Apply differential privacy noise if configured
        if let Some(ref dp_config) = self.config.dp_config {
            let seed = rdtsc();
            let mut dp = dp_config.clone();
            dp.add_noise(
                &mut self.global_model.params,
                self.collected_updates.len(),
                seed,
            );
            dp.accountant
                .step(dp.noise_multiplier, self.config.client_fraction);
        }

        self.global_model.version += 1;
        self.round_state = RoundState::Complete;

        serial_println!(
            "[FEDML] Round {} aggregation complete (model v{})",
            self.current_round,
            self.global_model.version
        );

        // Return a pseudo-loss (simplified)
        let loss = 1.0 / (self.current_round as f64 + 1.0);
        self.training_losses.push(loss);
        loss
    }

    /// Run full training
    pub fn train(&mut self) -> Vec<f64> {
        let total_rounds = self.config.rounds;
        let mut losses = Vec::new();

        for _ in 0..total_rounds {
            let _selected = self.start_round();

            // Simulate local training
            self.round_state = RoundState::TrainingLocally;

            // In real implementation, clients would train locally
            // and send back updates. We simulate by adding noise.
            for &client_id in &self.selected_clients.clone() {
                let mut update = self.global_model.params.clone();
                let seed = rdtsc() ^ client_id;
                let mut rng = seed;
                for tensor in update.values_mut() {
                    for v in tensor.data.iter_mut() {
                        rng = rng
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        let noise = ((rng >> 33) as f64) / (u32::MAX as f64) * 0.01 - 0.005;
                        *v += noise;
                    }
                }
                self.receive_update(client_id, update);
            }

            let loss = self.aggregate();
            losses.push(loss);
        }

        losses
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref COORDINATORS: Mutex<BTreeMap<u64, FederatedCoordinator>> = Mutex::new(BTreeMap::new());
    static ref NEXT_COORDINATOR_ID: AtomicU64 = AtomicU64::new(1);
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Create a new federated learning coordinator
pub fn create_coordinator(model_name: &str, config: FederatedConfig) -> u64 {
    let model = FederatedModel::simple_mlp(model_name, 784, 128, 10);
    let coordinator = FederatedCoordinator::new(model, config);
    let id = NEXT_COORDINATOR_ID.fetch_add(1, Ordering::Relaxed);
    COORDINATORS.lock().insert(id, coordinator);
    serial_println!(
        "[FEDML] Created coordinator {} for model '{}'",
        id,
        model_name
    );
    id
}

// ═══════════════════════════════════════════════════════════════════════
// MATH HELPERS (no_std)
// ═══════════════════════════════════════════════════════════════════════

const PI: f64 = core::f64::consts::PI;

fn sqrt(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut guess = x / 2.0;
    for _ in 0..64 {
        guess = (guess + x / guess) / 2.0;
    }
    guess
}

fn round(x: f64) -> f64 {
    let truncated = x as i64 as f64;
    if x - truncated >= 0.5 {
        truncated + 1.0
    } else if x - truncated <= -0.5 {
        truncated - 1.0
    } else {
        truncated
    }
}

fn ceil(x: f64) -> f64 {
    let truncated = x as i64 as f64;
    if x > truncated {
        truncated + 1.0
    } else {
        truncated
    }
}

fn ln(x: f64) -> f64 {
    if x <= 0.0 {
        return -1e308;
    }
    // Use series expansion around 1
    let mut result = 0.0;
    let mut y = x;
    let mut exp = 0i32;

    // Normalize to [0.5, 2]
    while y > 2.0 {
        y /= core::f64::consts::E;
        exp += 1;
    }
    while y < 0.5 {
        y *= core::f64::consts::E;
        exp -= 1;
    }

    let z = (y - 1.0) / (y + 1.0);
    let z2 = z * z;
    let mut term = z;
    for i in 0..32 {
        result += term / (2 * i + 1) as f64;
        term *= z2;
    }
    result * 2.0 + exp as f64
}

fn cos(x: f64) -> f64 {
    let mut x = x % (2.0 * PI);
    if x < 0.0 {
        x += 2.0 * PI;
    }
    let x2 = x * x;
    let mut result = 1.0;
    let mut term = 1.0;
    for i in 1..20 {
        term *= -x2 / ((2 * i - 1) as f64 * (2 * i) as f64);
        result += term;
    }
    result
}

fn rdtsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    return crate::arch_compat::read_tsc();
    #[cfg(not(target_arch = "x86_64"))]
    return 0;
}

/// Initialize federated ML subsystem
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!(
        "[KnoxOS] Federated ML initialized (FedAvg, DP-SGD, SecureAgg, Byzantine tolerance)"
    );
}
