#![allow(dead_code)] // TODO

use std::iter;

use ndarray::Axis;

use super::{ activations::*, common::*, losses::*, wb_initializers::* };


pub struct Network {
    sizes: Vec<usize>,
    biases: Vec<A1>,
    weights: Vec<A2>,
}


impl Network {
    pub fn new(sizes: Vec<usize>) -> Self {
        type WBInit = WBInitializerDefault;
        let biases = (&sizes[1..]).iter().map(|&s| {
            WBInit::make_biases(s)
        }).collect();
        let weights = (&sizes[..(sizes.len() - 1)]).iter().zip((&sizes[1..]).iter()).map(
            |(&x, &y)| WBInit::make_weights(y, x)
        ).collect();

        Network {
            sizes,
            biases,
            weights,
        }
    }

    fn feedforward<N: Activation>(&self, inputs: V2) -> A2 {
        let a0 = inputs.t().into_owned();
        iter::zip(self.biases.iter(), self.weights.iter()).fold(
            a0,
            |a, (b, w)| { N::call(&Self::make_weighted_inputs(&a, w, b)) }
        )
    }

    pub fn sgd(
        &mut self,
        training_data: TrainingData,
        epochs: usize,
        mini_batch_size: usize,
        eta: f32,
        test_data: Option<ValidationData>,
    ) {
        let data_size = training_data.len();
        let num_of_batches = data_size / mini_batch_size;
        println!("Number of training samples: {}", data_size);
        println!("Number of mini-batches: {}", num_of_batches);
        for i in 0..epochs {
            println!("====== Epoch {} started ======", i);

            training_data.iter(mini_batch_size).for_each(
                |(samples, truths)| self.update_mini_batch((samples, truths), eta)
            );
            match &test_data {
                Some(data) => {
                    println!(
                        "====== Epoch {}: {} / {} ======",
                        i,
                        self.evaluate(data, mini_batch_size),
                        data.len(),
                    );
                },
                None => println!("====== Epoch {} complete ======", i),
            }
        }
    }

    fn update_mini_batch(&mut self, mini_batch: (A2, A2), eta: f32) {
        let batch_size = mini_batch.0.len_of(Axis(0));
        let scale = eta / (batch_size as f32);

        let (nabla_b, nabla_w) = self.backprop::<Sigmoid, CrossEntropyLoss>(mini_batch);

        self.biases.iter_mut().zip(nabla_b).for_each(|(b, nb)| *b -= &(scale * nb));
        self.weights.iter_mut().zip(nabla_w).for_each(|(w, nw)| *w -= &(scale * nw));
    }

    // Return the gradients of weights and biases for all the layers computed
    // with all the samples in the batch.
    // A weight gradient for a single layer is a JxK matrix, while a biase
    // graident is a J-sized column vector.
    fn backprop<N, L>(&self, inputs: (A2, A2)) -> (Vec<A1>, Vec<A2>)
    where
        N: Activation,
        L: Loss,
    {
        let mut nabla_b = Vec::<A2>::new();
        let mut nabla_w = Vec::<A2>::new();
        let samples = inputs.0.reversed_axes();
        let truths = inputs.1.reversed_axes();

        // feedforward
        //
        // Both activations and weighted inputs are kept with an JxN matrix,
        // where:
        //   J is the number of neurons in the layer
        //   N is the batch size
        let mut activations = vec![samples];
        let mut zs = Vec::<A2>::new();
        for (b, w) in iter::zip(self.biases.iter(), self.weights.iter()) {
            let z = Self::make_weighted_inputs(&activations.last().unwrap(), w, b);
            activations.push(N::call(&z));
            zs.push(z);
        }

        // backward pass
        //
        // A JxN matrix
        let last_delta = L::delta::<N>(
            activations.pop().unwrap(), &truths.view(), zs.last().unwrap()
        );
        // Activation's size is KxN, so weight graident has a size of JxN * NxK => JxK
        //   Note:
        //     Not only does the matrix multiplication compute gradients with
        //     all the samples at the same time, it also sum the resulting
        //     gradients for every element of the weight matrix.
        nabla_w.push(last_delta.dot(&activations.pop().unwrap().reversed_axes()));
        nabla_b.push(last_delta);
        for l in 2..self.num_layers() {
            let z = &zs[zs.len() - l];
            let sp = N::prime(z);
            let delta = self.weights[self.weights.len() - l + 1].t().dot(nabla_b.last().unwrap()) * sp;
            nabla_w.push(delta.dot(&activations.pop().unwrap().reversed_axes()));
            nabla_b.push(delta);
        }

        // Sum nabla_b (nabla_w were already summed when performing matrix multiplications above)
        (
            nabla_b.iter().map(|nb| nb.sum_axis(Axis(1))).rev().collect(),
            nabla_w.into_iter().rev().collect()
        )
    }

    fn evaluate(&self, test_data: &ValidationData, batch_size: usize) -> usize {
        test_data.iter(batch_size)
            .map(|(samples, truths)| {
                let mut corrected = 0;
                for (output, label) in iter::zip(
                    self.feedforward::<Sigmoid>(samples).columns(), truths
                ) {
                    let idx_of_max_prob = output.t().into_iter()
                        .enumerate()
                        // No Infs and NaNs so we can simply use partial_cmp() here
                        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                        .map(|(idx, _)| idx).unwrap();
                    corrected += (idx_of_max_prob == (*label as usize)) as usize;

                }
                corrected
            })
            .sum()
    }

    fn make_weighted_inputs(inputs: &A2, weights: &A2, biases: &A1) -> A2 {
        // Turn biases into a column vector and broadcast it
        weights.dot(inputs) + biases.to_shape((biases.len(), 1)).unwrap()
    }

    fn num_layers(&self) -> usize {
        self.sizes.len()
    }
}
