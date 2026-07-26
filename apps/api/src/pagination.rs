use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PageResponse<T> {
    pub content: Vec<T>,
    pub page: u32,
    pub size: u32,
    pub total_elements: i64,
    pub total_pages: i64,
}

impl<T> PageResponse<T> {
    #[must_use]
    pub fn new(content: Vec<T>, page: u32, size: u32, total_elements: i64) -> Self {
        let size_i64 = i64::from(size);
        let total_pages =
            if size_i64 == 0 { 0 } else { total_elements.saturating_add(size_i64 - 1) / size_i64 };
        Self { content, page, size, total_elements, total_pages }
    }
}

pub fn checked_offset(page: u32, size: u32) -> Result<i64, crate::error::AppError> {
    i64::from(page)
        .checked_mul(i64::from(size))
        .ok_or_else(|| crate::error::AppError::validation("page", "is too large"))
}

#[cfg(test)]
mod tests {
    use super::{PageResponse, checked_offset};

    #[test]
    fn zero_page_size_does_not_panic() {
        let page = PageResponse::<u8>::new(Vec::new(), 0, 0, 10);
        assert_eq!(page.total_pages, 0);
    }

    #[test]
    fn offset_overflow_is_rejected() {
        assert!(checked_offset(u32::MAX, u32::MAX).is_err());
    }
}
