use super::{construct_intermediate_sets, ChallengeV, Query};
use crate::arithmetic::{eval_polynomial, kate_division, CurveAffine, FieldExt};
use crate::poly::multiopen::ProverQuery;
use crate::poly::Rotation;
use crate::poly::{commitment::Params, Coeff, Polynomial};
use crate::transcript::{EncodedChallenge, TranscriptWrite};

use ff::Field;
use group::Curve;
use rayon::iter::*;
use std::io;
use std::marker::PhantomData;
use std::sync::Mutex;

/// Create a multi-opening proof
pub fn create_proof<'a, I, C: CurveAffine, E: EncodedChallenge<C>, T: TranscriptWrite<C, E>>(
    params: &Params<C>,
    transcript: &mut T,
    queries: I,
) -> io::Result<()>
where
    I: IntoIterator<Item = ProverQuery<'a, C>> + Clone,
{
    let v: ChallengeV<_> = transcript.squeeze_challenge_scalar();
    let commitment_data = construct_intermediate_sets(queries);

    let zero = || Polynomial::<C::Scalar, Coeff> {
        values: vec![C::Scalar::ZERO; params.n as usize],
        _marker: PhantomData,
    };

    let mut ws = vec![C::identity(); commitment_data.len()];

    let lock = Mutex::new(0);

    commitment_data
        .par_iter()
        .zip(ws.par_iter_mut())
        .for_each(|(commitment_at_a_point, w)| {
            let mut poly_batch = zero();
            let mut eval_batch = C::Scalar::ZERO;
            let z = commitment_at_a_point.point;
            for query in commitment_at_a_point.queries.iter() {
                assert_eq!(query.get_point(), z);

                let poly = query.get_commitment().poly;
                let eval = query.get_eval();
                poly_batch = poly_batch * *v + poly;
                eval_batch = eval_batch * *v + eval;
            }

            let poly_batch = &poly_batch - eval_batch;
            let witness_poly = Polynomial {
                values: kate_division(&poly_batch.values, z),
                _marker: PhantomData,
            };

            let _guard = lock.lock().unwrap();
            *w = params.commit(&witness_poly).to_affine();
        });

    for w in ws {
        transcript.write_point(w)?;
    }

    Ok(())
}

#[doc(hidden)]
#[derive(Copy, Clone, Debug)]
pub struct PolynomialPointer<'a, C: CurveAffine> {
    poly: &'a Polynomial<C::Scalar, Coeff>,
}

impl<'a, C: CurveAffine> PartialEq for PolynomialPointer<'a, C> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.poly, other.poly)
    }
}

impl<'a, C: CurveAffine> Query<C::Scalar> for ProverQuery<'a, C> {
    type Commitment = PolynomialPointer<'a, C>;

    fn get_point(&self) -> C::Scalar {
        self.point
    }
    fn get_rotation(&self) -> Rotation {
        self.rotation
    }
    fn get_eval(&self) -> C::Scalar {
        eval_polynomial(self.poly, self.get_point())
    }
    fn get_commitment(&self) -> Self::Commitment {
        PolynomialPointer { poly: self.poly }
    }
}