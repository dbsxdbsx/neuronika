use super::{
    assert_almost_equals, new_backward_input, new_input, new_tensor, Backward, Cache, Data,
    Forward, Gradient, Logn, LognBackward, Overwrite, Tensor,
};

mod forward {
    use super::{assert_almost_equals, new_input, new_tensor, Cache, Data, Forward, Logn, Tensor};

    #[test]
    fn creation() {
        let input = new_input((3, 3), vec![1., 2., 3., 4., 5., 6., 7., 8., 9.]);
        let node = Logn::new(input);

        assert_eq!(*node.data(), Tensor::from_elem((3, 3), 0.));
        assert_eq!(*node.data_mut(), Tensor::from_elem((3, 3), 0.));
        assert!(!node.was_computed());
    }

    #[test]
    fn computation_was_computed_transition() {
        let input = new_input((3, 3), vec![1., 2., 3., 4., 5., 6., 7., 8., 9.]);
        let node = Logn::new(input);

        node.forward();
        assert!(node.was_computed());

        node.forward();
        assert!(node.was_computed());

        node.reset_computation();
        assert!(!node.was_computed());

        node.reset_computation();
        assert!(!node.was_computed());
    }

    #[allow(clippy::approx_constant)]
    #[test]
    fn forward() {
        let input = new_input((3, 3), vec![1., 2., 3., 4., 5., 6., 7., 8., 9.]);
        let node = Logn::new(input.clone());

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ First Evaluation ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        node.forward();
        assert_almost_equals(
            &*node.data(),
            &new_tensor(
                (3, 3),
                vec![
                    0., 0.6931, 1.0986, 1.3863, 1.6094, 1.7918, 1.9459, 2.0794, 2.1972,
                ],
            ),
        );

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ No Second Evaluation ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        {
            let mut data = input.data_mut();
            *data = &*data + &Tensor::from_elem(1, 1.);
        }
        assert_almost_equals(
            &*input.data(),
            &new_tensor((3, 3), vec![2., 3., 4., 5., 6., 7., 8., 9., 10.]),
        );

        node.forward();
        assert_almost_equals(
            &*node.data(),
            &new_tensor(
                (3, 3),
                vec![
                    0., 0.6931, 1.0986, 1.3863, 1.6094, 1.7918, 1.9459, 2.0794, 2.1972,
                ],
            ),
        );

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Second Evaluation ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        node.reset_computation();
        node.forward();
        assert_almost_equals(
            &*node.data(),
            &new_tensor(
                (3, 3),
                vec![
                    0.6931, 1.0986, 1.3863, 1.6094, 1.7918, 1.9459, 2.0794, 2.1972, 2.3026,
                ],
            ),
        );
    }

    #[test]
    fn debug() {
        let input = new_input((3, 3), vec![-4., -3., -2., -1., 0., 1., 2., 3., 4.]);
        let node = Logn::new(input);

        let output = "Logn { data: [[0.0, 0.0, 0.0],\n [0.0, 0.0, 0.0],\n [0.0, 0.0, 0.0]], shape=[3, 3], strides=[3, 1], layout=Cc (0x5), const ndim=2, computed: false }";

        assert_eq!(output, format!("{:?}", node));
    }

    #[test]
    fn display() {
        let input = new_input((3, 3), vec![-4., -3., -2., -1., 0., 1., 2., 3., 4.]);
        let node = Logn::new(input);

        assert_eq!(format!("{}", node.data()), format!("{}", node));
    }
}

mod backward {
    use super::{
        assert_almost_equals, new_backward_input, new_input, new_tensor, Backward, Gradient,
        LognBackward, Overwrite, Tensor,
    };

    #[test]
    fn creation() {
        let node = LognBackward::new(
            new_backward_input(3, vec![0.; 3]),
            new_input(3, vec![1., 2., 3.]),
        );

        assert_eq!(*node.gradient(), Tensor::from_elem(3, 0.));
        assert_eq!(*node.gradient_mut(), Tensor::from_elem(3, 0.));
        assert!(node.can_overwrite());
    }

    #[test]
    fn computation_state_transition() {
        let diff = new_backward_input(3, vec![0.; 3]);
        let node = LognBackward::new(diff.clone(), new_input(3, vec![1., 2., 3.]));

        node.backward();
        assert!(node.can_overwrite());
        assert!(!diff.can_overwrite());

        node.backward();
        assert!(node.can_overwrite());
        assert!(!diff.can_overwrite());

        diff.set_overwrite(true);
        assert!(node.can_overwrite());
        assert!(diff.can_overwrite());

        diff.set_overwrite(true);
        assert!(node.can_overwrite());
        assert!(diff.can_overwrite());

        node.set_overwrite(false);
        assert!(!node.can_overwrite());
        assert!(diff.can_overwrite());

        node.set_overwrite(false);
        assert!(!node.can_overwrite());
        assert!(diff.can_overwrite());

        node.backward();
        assert!(!node.can_overwrite());
        assert!(!diff.can_overwrite());

        node.backward();
        assert!(!node.can_overwrite());
        assert!(!diff.can_overwrite());
    }

    #[test]
    fn backward() {
        let diff = new_backward_input(3, vec![0.; 3]);
        let node = LognBackward::new(diff.clone(), new_input(3, vec![1., 2., 3.]));

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Seed Gradient ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        *node.gradient_mut() = new_tensor(3, vec![1.; 3]);
        assert_almost_equals(&*node.gradient(), &new_tensor(3, vec![1.; 3]));

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ First Evaluation ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        node.backward();
        assert_almost_equals(&*diff.gradient(), &new_tensor(3, vec![1., 0.5, 0.33333]));

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Second Evaluation ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        node.backward();
        assert_almost_equals(&*diff.gradient(), &new_tensor(3, vec![2., 1., 0.66667]));

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ Third Evaluation ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        diff.set_overwrite(true);
        node.set_overwrite(true);
        node.backward();
        assert_almost_equals(&*diff.gradient(), &new_tensor(3, vec![1., 0.5, 0.33333]));
    }

    #[test]
    fn no_grad() {
        // LognBackward
        let node = LognBackward::new(
            new_backward_input((3, 3), vec![0.; 9]),
            new_input((3, 3), vec![0.; 9]),
        );

        node.no_grad();
        assert!(node.gradient.borrow().is_none());

        node.with_grad();
        assert_eq!(&*node.gradient(), Tensor::zeros(node.shape));
    }

    #[test]
    fn debug() {
        let node = LognBackward::new(
            new_backward_input((3, 3), vec![0.; 9]),
            new_input((3, 3), vec![0.; 9]),
        );

        let output = "LognBackward { gradient: Some([[0.0, 0.0, 0.0],\n [0.0, 0.0, 0.0],\n [0.0, 0.0, 0.0]], shape=[3, 3], strides=[3, 1], layout=Cc (0x5), const ndim=2), overwrite: true }";

        assert_eq!(output, format!("{:?}", node));
    }

    #[test]
    fn display() {
        let node = LognBackward::new(
            new_backward_input((3, 3), vec![0.; 9]),
            new_input((3, 3), vec![0.; 9]),
        );

        assert_eq!(format!("{}", node.gradient()), format!("{}", node));
    }
}
