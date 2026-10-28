// Run with --release: RUSTSEC-2024-0429 can manifest only under optimization.
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use glib::prelude::*;

    #[test]
    fn variant_strings_iterate_from_both_ends() {
        let strings = ["first", "", "caffè", "last"];
        let variant = strings.to_variant();

        assert_eq!(
            variant.array_iter_str().unwrap().collect::<Vec<_>>(),
            strings
        );
        assert_eq!(
            variant.array_iter_str().unwrap().rev().collect::<Vec<_>>(),
            strings.into_iter().rev().collect::<Vec<_>>()
        );

        let mut iter = variant.array_iter_str().unwrap();
        assert_eq!(iter.next(), Some("first"));
        assert_eq!(iter.next_back(), Some("last"));
        assert_eq!(iter.next_back(), Some("caffè"));
        assert_eq!(iter.next(), Some(""));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
    }

    #[test]
    fn variant_strings_support_indexed_access_and_last() {
        let variant = ["first", "middle", "last"].to_variant();

        assert_eq!(variant.array_iter_str().unwrap().nth(1), Some("middle"));
        assert_eq!(
            variant.array_iter_str().unwrap().nth_back(1),
            Some("middle")
        );
        assert_eq!(variant.array_iter_str().unwrap().last(), Some("last"));
        assert_eq!(variant.array_iter_str().unwrap().nth(3), None);
        assert_eq!(variant.array_iter_str().unwrap().nth_back(3), None);

        let empty = Vec::<String>::new().to_variant();
        assert_eq!(empty.array_iter_str().unwrap().next(), None);
        assert_eq!(empty.array_iter_str().unwrap().next_back(), None);
        assert_eq!(empty.array_iter_str().unwrap().last(), None);
    }

    #[test]
    fn pointer_slice_truncates_and_remains_reusable() {
        let items = [
            glib::Error::new(glib::FileError::Failed, "first"),
            glib::Error::new(glib::FileError::Failed, "middle"),
            glib::Error::new(glib::FileError::Failed, "last"),
        ];
        let mut slice = glib::PtrSlice::from(&items[..]);
        slice.truncate(5);
        assert_eq!(slice.len(), 3);
        slice.truncate(1);
        assert_eq!(slice.len(), 1);
        assert_eq!(slice[0].message(), "first");
        slice.push(items[2].clone());
        assert_eq!(slice.pop().unwrap().message(), "last");
        assert_eq!(slice.pop().unwrap().message(), "first");
        assert!(slice.pop().is_none());
        slice.truncate(0);
        slice.push(items[1].clone());
        slice.truncate(0);
        assert!(slice.is_empty());
        slice.push(items[0].clone());
        assert_eq!(slice[0].message(), "first");
    }
}
