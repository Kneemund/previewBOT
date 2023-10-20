const beforeImage = document.querySelector('.image-comparison .before-image');
const afterImage = document.querySelector('.image-comparison .after-image');

const beforeLabel = document.querySelector('.image-comparison .before-label');
const afterLabel = document.querySelector('.image-comparison .after-label');

const beforeLabelText = beforeLabel.firstElementChild;
const afterLabelText = afterLabel.firstElementChild;

const urlSearchParams = new URLSearchParams(window.location.search);
const parameters = {
    data: urlSearchParams.get('d'),
    mac: urlSearchParams.get('m'),
    orientation: urlSearchParams.get('o'),
};

if (!parameters.data || !parameters.mac || !parameters.orientation) {
    throw new Error('Missing required query parameters.');
}

fetch(`http://127.0.0.1:3000/v1/url?d=${parameters.data}&m=${parameters.mac}`)
    .then((data) => data.json())
    .then((data) => {
        beforeImage.src = data.left_image_url;
        afterImage.src = data.right_image_url;

        if (data.left_image_label) {
            beforeLabelText.textContent = data.left_image_label;
            beforeLabel.style.display = 'block';
        }

        if (data.right_image_label) {
            afterLabelText.textContent = data.right_image_label;
            afterLabel.style.display = 'block';
        }
    });

const slider = document.querySelector('.image-comparison .slider');
const sliderLine = document.querySelector('.image-comparison .slider-line');
const sliderIcon = document.querySelector('.image-comparison .slider-icon');

slider.addEventListener('input', (e) => {
    const sliderValue = e.target.value + '%';
    const inverseSliderValue = 100 - e.target.value + '%';

    beforeLabel.style['max-width'] = sliderValue;
    afterLabel.style['max-width'] = inverseSliderValue;

    beforeImage.style.width = sliderValue;
    sliderLine.style.left = sliderValue;
    sliderIcon.style.left = sliderValue;
});

slider.addEventListener('mouseup', (e) => {
    e.target.blur();
});
