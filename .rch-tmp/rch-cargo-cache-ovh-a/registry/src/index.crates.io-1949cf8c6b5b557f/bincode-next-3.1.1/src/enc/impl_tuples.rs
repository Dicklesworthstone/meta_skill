use super::Encode;
use super::Encoder;
use crate::error::EncodeError;

impl<A> Encode for (A,)
where
    A: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        Ok(())
    }
}

impl<A, B> Encode for (A, B)
where
    A: Encode,
    B: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C> Encode for (A, B, C)
where
    A: Encode,
    B: Encode,
    C: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D> Encode for (A, B, C, D)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E> Encode for (A, B, C, D, E)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F> Encode for (A, B, C, D, E, F)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G> Encode for (A, B, C, D, E, F, G)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H> Encode for (A, B, C, D, E, F, G, H)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I> Encode for (A, B, C, D, E, F, G, H, I)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I, J> Encode for (A, B, C, D, E, F, G, H, I, J)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        self.9.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I, J, K> Encode for (A, B, C, D, E, F, G, H, I, J, K)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
    K: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        self.9.encode(encoder)?;
        self.10.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I, J, K, L> Encode for (A, B, C, D, E, F, G, H, I, J, K, L)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
    K: Encode,
    L: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        self.9.encode(encoder)?;
        self.10.encode(encoder)?;
        self.11.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I, J, K, L, M> Encode for (A, B, C, D, E, F, G, H, I, J, K, L, M)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
    K: Encode,
    L: Encode,
    M: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        self.9.encode(encoder)?;
        self.10.encode(encoder)?;
        self.11.encode(encoder)?;
        self.12.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I, J, K, L, M, N> Encode for (A, B, C, D, E, F, G, H, I, J, K, L, M, N)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
    K: Encode,
    L: Encode,
    M: Encode,
    N: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        self.9.encode(encoder)?;
        self.10.encode(encoder)?;
        self.11.encode(encoder)?;
        self.12.encode(encoder)?;
        self.13.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I, J, K, L, M, N, O> Encode
    for (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
    K: Encode,
    L: Encode,
    M: Encode,
    N: Encode,
    O: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        self.9.encode(encoder)?;
        self.10.encode(encoder)?;
        self.11.encode(encoder)?;
        self.12.encode(encoder)?;
        self.13.encode(encoder)?;
        self.14.encode(encoder)?;
        Ok(())
    }
}

impl<A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P> Encode
    for (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P)
where
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
    K: Encode,
    L: Encode,
    M: Encode,
    N: Encode,
    O: Encode,
    P: Encode,
{
    #[inline(always)]
    fn encode<_E: Encoder>(
        &self,
        encoder: &mut _E,
    ) -> Result<(), EncodeError> {
        self.0.encode(encoder)?;
        self.1.encode(encoder)?;
        self.2.encode(encoder)?;
        self.3.encode(encoder)?;
        self.4.encode(encoder)?;
        self.5.encode(encoder)?;
        self.6.encode(encoder)?;
        self.7.encode(encoder)?;
        self.8.encode(encoder)?;
        self.9.encode(encoder)?;
        self.10.encode(encoder)?;
        self.11.encode(encoder)?;
        self.12.encode(encoder)?;
        self.13.encode(encoder)?;
        self.14.encode(encoder)?;
        self.15.encode(encoder)?;
        Ok(())
    }
}
